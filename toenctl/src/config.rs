use std::fs;
use std::path::Path;

use crate::ProjectConfigSchema;

pub(crate) fn load(root: &Path) -> Result<ProjectConfigSchema, String> {
    let text = fs::read_to_string(root.join("toen.toml"))
        .map_err(|error| format!("read toen.toml: {error}"))?;
    let config: ProjectConfigSchema =
        toml::from_str(&text).map_err(|error| format!("parse toen.toml: {error}"))?;

    if config.schema_version != 1
        || config.accepted_records == 0
        || config.runtime_core.ammodino == 0
        || config.runtime_core.arranda == 0
        || config.integrations.supported.is_empty()
    {
        return Err(
            "toen.toml has invalid schema, corpus, runtime, or integration settings".to_owned(),
        );
    }
    if config.tokenizer.id != crate::toenizer::TOKENIZER_ID {
        return Err(format!(
            "toen.toml selects unsupported tokenizer {}; only {} is available",
            config.tokenizer.id,
            crate::toenizer::TOKENIZER_ID
        ));
    }
    for (name, budget) in [
        ("portable", &config.budgets.portable),
        ("codex", &config.budgets.codex),
        ("claude-code", &config.budgets.claude_code),
    ] {
        if budget.tokens == 0 || budget.utf8_bytes == 0 || budget.lines == 0 {
            return Err(format!("toen.toml has invalid {name} budget"));
        }
    }
    if config.generated.assets.is_empty() {
        return Err("toen.toml must declare generated assets".to_owned());
    }

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn missing_typed_budget_is_a_configuration_error() {
        let root = std::env::temp_dir().join(format!("toen-config-{}", std::process::id()));
        fs::create_dir_all(&root).unwrap();
        fs::write(
            root.join("toen.toml"),
            "schema_version = 1\naccepted_records = 1\n\n[runtime_core]\nammodino = 1\narranda = 1\n\n[tokenizer]\nid = \"o200k-base\"\n\n[budgets.portable]\ntokens = 1\nutf8_bytes = 1\nlines = 1\n\n[budgets.codex]\ntokens = 1\nutf8_bytes = 1\nlines = 1\n\n[generated]\nassets = [\"generated\"]\n\n[integrations]\nsupported = [\"portable\"]\n",
        )
        .unwrap();

        let error = load(&root).unwrap_err();
        assert!(error.contains("parse toen.toml"));
        fs::remove_dir_all(root).unwrap();
    }
}
