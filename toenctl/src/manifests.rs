//! Host manifest validation is kept behind the maintainer CLI boundary.

use std::fs;
use std::path::Path;

use serde_json::Value;

use crate::{VERSION, read_json};

pub(crate) fn check(root: &Path) -> Result<(), String> {
    let version = fs::read_to_string(root.join("VERSION"))
        .map_err(|error| format!("read VERSION: {error}"))?;

    if version.trim() != VERSION {
        return Err("VERSION does not match the maintainer binary version".to_owned());
    }

    let distributions = [
        "skill/toen",
        "plugins/codex/toen",
        "plugins/claude-code/toen",
    ];
    for distribution in distributions {
        for legal_file in ["LICENSE", "CORPUS-LICENSE.md"] {
            let root_contents = fs::read(root.join(legal_file))
                .map_err(|error| format!("read {legal_file}: {error}"))?;
            let distribution_path = root.join(distribution).join(legal_file);
            let distribution_contents = fs::read(&distribution_path)
                .map_err(|error| format!("read {}: {error}", distribution_path.display()))?;
            if root_contents != distribution_contents {
                return Err(format!(
                    "{} must match {legal_file}",
                    distribution_path.display()
                ));
            }
        }
        let source_notice = root.join(distribution).join("SOURCE-NOTICE.md");
        let expected_notice = fs::read(root.join("docs/source-notice.md"))
            .map_err(|error| format!("read docs/source-notice.md: {error}"))?;
        if fs::read(&source_notice)
            .map_err(|error| format!("read {}: {error}", source_notice.display()))?
            != expected_notice
        {
            return Err(format!(
                "{} must match docs/source-notice.md",
                source_notice.display()
            ));
        }
    }

    if root.join("plugins/toen").exists() {
        return Err("stale plugins/toen directory must be removed".to_owned());
    }

    let portable = fs::read_to_string(root.join("skill/toen/SKILL.md"))
        .map_err(|error| format!("read portable skill: {error}"))?;
    let codex_skill = fs::read_to_string(root.join("plugins/codex/toen/skills/toen/SKILL.md"))
        .map_err(|error| format!("read Codex skill: {error}"))?;
    if portable != codex_skill {
        return Err("portable and Codex skills must be byte-identical".to_owned());
    }
    let claude_skill =
        fs::read_to_string(root.join("plugins/claude-code/toen/skills/toen/SKILL.md"))
            .map_err(|error| format!("read Claude Code skill: {error}"))?;
    for required in [
        "disable-model-invocation: true",
        "argument-hint: \"[ammodino|arranda|de|spengi] [task]\"",
    ] {
        if !claude_skill.contains(required) {
            return Err(format!("Claude Code skill is missing `{required}`"));
        }
    }

    let plugin = read_json(&root.join("plugins/codex/toen/.codex-plugin/plugin.json"))?;
    let plugin_name = plugin.get("name").and_then(Value::as_str);
    let plugin_version = plugin.get("version").and_then(Value::as_str);
    let skills = plugin.get("skills").and_then(Value::as_str);
    let interface = plugin.get("interface").and_then(Value::as_object);
    if plugin_name != Some("toen")
        || plugin_version != Some(VERSION)
        || skills != Some("./skills/")
        || interface.is_none_or(|value| {
            value.get("displayName").and_then(Value::as_str) != Some("Toen")
                || value
                    .get("shortDescription")
                    .and_then(Value::as_str)
                    .is_none_or(str::is_empty)
        })
    {
        return Err("plugin manifest has invalid identity, metadata, or skills path".to_owned());
    }

    let policy = fs::read_to_string(root.join("plugins/codex/toen/skills/toen/agents/openai.yaml"))
        .map_err(|error| format!("read skill policy: {error}"))?;
    if !has_explicit_invocation_policy(&policy) {
        return Err("skill policy must disable implicit invocation".to_owned());
    }

    let marketplace = read_json(&root.join(".agents/plugins/marketplace.json"))?;
    let marketplace_name = marketplace.get("name").and_then(Value::as_str);
    let plugins = marketplace
        .get("plugins")
        .and_then(Value::as_array)
        .ok_or_else(|| "marketplace is missing plugins[]".to_owned())?;
    let entry = plugins
        .iter()
        .find(|entry| entry.get("name").and_then(Value::as_str) == Some("toen"))
        .ok_or_else(|| "marketplace is missing the toen entry".to_owned())?;
    let path = entry
        .get("source")
        .and_then(|source| source.get("path"))
        .and_then(Value::as_str);
    let source_kind = entry
        .get("source")
        .and_then(|source| source.get("source"))
        .and_then(Value::as_str);
    let marketplace_version = entry.get("version").and_then(Value::as_str);
    if marketplace_name != Some("toen")
        || source_kind != Some("local")
        || path != Some("./plugins/codex/toen")
        || marketplace_version != Some(VERSION)
    {
        return Err("marketplace toen entry has the wrong source path".to_owned());
    }

    let installation = entry
        .get("policy")
        .and_then(|policy| policy.get("installation"))
        .and_then(Value::as_str);
    let authentication = entry
        .get("policy")
        .and_then(|policy| policy.get("authentication"))
        .and_then(Value::as_str);
    let category = entry.get("category").and_then(Value::as_str);
    if installation != Some("AVAILABLE")
        || authentication != Some("ON_INSTALL")
        || category != Some("Productivity")
    {
        return Err("marketplace toen entry has invalid policy metadata".to_owned());
    }

    let claude_manifest =
        read_json(&root.join("plugins/claude-code/toen/.claude-plugin/plugin.json"))?;
    if claude_manifest.get("name").and_then(Value::as_str) != Some("toen")
        || claude_manifest.get("version").and_then(Value::as_str) != Some(VERSION)
        || claude_manifest.get("skills").and_then(Value::as_str) != Some("./skills/")
    {
        return Err(
            "Claude Code plugin manifest has invalid identity, version, or skills path".to_owned(),
        );
    }

    let claude_marketplace = read_json(&root.join(".claude-plugin/marketplace.json"))?;
    if claude_marketplace.get("name").and_then(Value::as_str) != Some("toen") {
        return Err("Claude marketplace has the wrong name".to_owned());
    }
    let claude_entry = claude_marketplace
        .get("plugins")
        .and_then(Value::as_array)
        .and_then(|plugins| {
            plugins
                .iter()
                .find(|entry| entry.get("name").and_then(Value::as_str) == Some("toen"))
        })
        .ok_or_else(|| "Claude marketplace is missing the toen entry".to_owned())?;
    if claude_entry.get("source").and_then(Value::as_str) != Some("./plugins/claude-code/toen")
        || claude_entry.get("version").and_then(Value::as_str) != Some(VERSION)
    {
        return Err("Claude marketplace toen entry has the wrong source or version".to_owned());
    }

    println!("manifests: portable, Codex, Claude Code, and marketplace metadata passed validation");
    Ok(())
}

pub(crate) fn has_explicit_invocation_policy(yaml: &str) -> bool {
    let mut in_policy = false;
    for line in yaml.lines() {
        if line.trim().is_empty() || line.trim_start().starts_with('#') {
            continue;
        }
        if !line.starts_with(' ') {
            in_policy = line == "policy:";
            continue;
        }
        if in_policy && line == "  allow_implicit_invocation: false" {
            return true;
        }
    }
    false
}
