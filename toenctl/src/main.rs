mod bench;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tiktoken_rs::o200k_base;
use zip::ZipWriter;
use zip::write::FileOptions;

const VERSION: &str = "0.1.0";
const ACCEPTED_COUNT: usize = 500;
const AMMODINO_COUNT: usize = 50;
const ARRANDA_COUNT: usize = 30;
const SKILL_TOKEN_BUDGET: usize = 750;

#[derive(Debug, Deserialize, Serialize)]
struct Record {
    id: String,
    canonical: String,
    lemma: String,
    kind: String,
    gloss_it: String,
    gloss_en: String,
    register: String,
    grammatical_role: String,
    contemporary_status: String,
    confidence: String,
    variants: Vec<String>,
    usage_notes: String,
    allowed_modes: Vec<String>,
    runtime: String,
    runtime_priority: u32,
    examples: Vec<Example>,
    evidence: Vec<Evidence>,
    review: Review,
}

#[derive(Debug, Deserialize, Serialize)]
struct Example {
    livornese: String,
    italian: String,
}

#[derive(Debug, Deserialize, Serialize)]
struct Evidence {
    source_id: String,
    locator: String,
    url: String,
    accessed: String,
    archive_url: Option<String>,
}

#[derive(Debug, Deserialize, Serialize)]
struct Review {
    reviewer: String,
    date: String,
}

#[derive(Debug, Deserialize)]
struct Source {
    id: String,
    name: String,
    url: String,
    archive_url: Option<String>,
    local_attestation: bool,
}

#[derive(Debug, Deserialize)]
struct SourcesFile {
    source: Vec<Source>,
}

#[derive(Debug, Deserialize)]
struct GrammarFile {
    rule: Vec<GrammarRule>,
}

#[derive(Debug, Deserialize)]
struct GrammarRule {
    id: String,
    modes: Vec<String>,
    text: String,
    source_ids: Vec<String>,
}

#[derive(Debug)]
struct SourceMetadata {
    url: String,
    archive_url: Option<String>,
    local_attestation: bool,
}

#[cfg(test)]
#[derive(Debug, PartialEq, Eq)]
enum ToenCommand {
    Chooser,
    Activate(String),
    Status,
    Deactivate,
    ActivateAndTask(String, String),
    DeactivateAndTask(String),
    Usage,
}

fn main() {
    let result = run(env::args().skip(1).collect());

    if let Err(error) = result {
        eprintln!("toenctl: {error}");
        std::process::exit(1);
    }
}

fn run(args: Vec<String>) -> Result<(), String> {
    let command = args.first().map(String::as_str).unwrap_or("help");

    match command {
        "corpus" if args.len() == 2 && args[1] == "check" => corpus_check(),
        "sources" if args.len() == 2 && args[1] == "verify" => sources_verify(None),
        "sources" if args.len() == 3 && args[1] == "verify" && args[2] == "--metadata-only" => {
            sources_verify(Some("--metadata-only"))
        }
        "manifests" if args.len() == 2 && args[1] == "check" => manifests_check(),
        "generate" if args.len() == 1 => generate(false),
        "generate" if args.len() == 2 && args[1] == "--check" => generate(true),
        "bench" => bench::run(&repo_root()?, &args[1..], VERSION),
        "package" => package(&args[1..]),
        "version" if args.len() == 1 => {
            println!("toenctl {VERSION}");
            Ok(())
        }
        "help" | "--help" | "-h" if args.len() == 1 => usage(),
        "corpus" | "sources" | "manifests" | "generate" | "version" => Err(format!(
            "invalid arguments for `{command}`; try `toenctl help`"
        )),
        _ => Err(format!("unknown command `{command}`; try `toenctl help`")),
    }
}

fn usage() -> Result<(), String> {
    println!(
        "toenctl {VERSION}\n\nCommands:\n  corpus check\n  sources verify [--metadata-only]\n  manifests check\n  generate [--check]\n  bench smoke|run|judge|report\n  package --version <version>"
    );
    Ok(())
}

fn corpus_check() -> Result<(), String> {
    let root = repo_root()?;
    let records = load_records(&root)?;

    if records.len() != ACCEPTED_COUNT {
        return Err(format!(
            "expected {ACCEPTED_COUNT} accepted records, found {}",
            records.len()
        ));
    }

    for (index, record) in records.iter().enumerate() {
        let expected = format!("liv-{:04}", index + 1);

        if record.id != expected {
            return Err(format!(
                "accepted record IDs must be contiguous; expected {expected}, found {}",
                record.id
            ));
        }
    }

    validate_record_relationships(&records)?;

    let ammodino = records
        .iter()
        .filter(|record| record.runtime == "ammodino")
        .count();
    let arranda = records
        .iter()
        .filter(|record| record.runtime == "arranda")
        .count();

    if ammodino != AMMODINO_COUNT || arranda != ARRANDA_COUNT {
        return Err(format!(
            "expected runtime core {AMMODINO_COUNT}/{ARRANDA_COUNT}, found {ammodino}/{arranda}"
        ));
    }

    validate_runtime_priorities(&records, "ammodino", AMMODINO_COUNT)?;
    validate_runtime_priorities(&records, "arranda", ARRANDA_COUNT)?;

    load_grammar(&root)?;

    println!(
        "corpus: {ACCEPTED_COUNT} accepted records; runtime core {ammodino} ammodino + {arranda} arranda"
    );
    Ok(())
}

fn load_records(root: &Path) -> Result<Vec<Record>, String> {
    let records_dir = root.join("corpus/accepted");
    let source_metadata = source_metadata(root)?;
    let mut records = Vec::new();

    for entry in fs::read_dir(&records_dir).map_err(|error| format!("read corpus: {error}"))? {
        let path = entry
            .map_err(|error| format!("read corpus entry: {error}"))?
            .path();

        if path.extension().and_then(|value| value.to_str()) != Some("toml") {
            continue;
        }

        let text = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        let record: Record =
            toml::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))?;

        validate_record(&record, &path, &source_metadata)?;

        records.push(record);
    }

    records.sort_by(|left, right| left.id.cmp(&right.id));
    Ok(records)
}

fn validate_record(
    record: &Record,
    path: &Path,
    source_metadata: &HashMap<String, SourceMetadata>,
) -> Result<(), String> {
    if [
        record.id.as_str(),
        record.canonical.as_str(),
        record.lemma.as_str(),
        record.gloss_it.as_str(),
        record.gloss_en.as_str(),
        record.usage_notes.as_str(),
        record.review.reviewer.as_str(),
    ]
    .iter()
    .any(|value| value.trim().is_empty())
    {
        return Err(format!("{} has an empty required field", path.display()));
    }

    validate_enum(path, "kind", &record.kind, &["lexeme", "idiom", "particle"])?;
    validate_enum(
        path,
        "register",
        &record.register,
        &["everyday", "colloquial", "expressive"],
    )?;
    validate_enum(
        path,
        "grammatical_role",
        &record.grammatical_role,
        &[
            "adjective",
            "adverb",
            "discourse",
            "interjection",
            "noun",
            "phrase",
            "pronoun",
            "verb",
        ],
    )?;
    validate_enum(
        path,
        "contemporary_status",
        &record.contemporary_status,
        &["current", "endangered"],
    )?;
    validate_enum(path, "confidence", &record.confidence, &["high", "medium"])?;

    if record.contemporary_status == "current" && record.confidence != "high" {
        return Err(format!(
            "{} current records must have high confidence",
            path.display()
        ));
    }

    if !["spento", "ammodino", "arranda"].contains(&record.runtime.as_str()) {
        return Err(format!("{} has an invalid runtime", path.display()));
    }

    if record.examples.is_empty() || record.evidence.is_empty() {
        return Err(format!(
            "{} needs an example, evidence, and review",
            path.display()
        ));
    }

    if record.allowed_modes.is_empty() {
        return Err(format!("{} needs allowed modes", path.display()));
    }

    if record.id.len() != 8 || !record.id.starts_with("liv-") {
        return Err(format!("{} has an invalid stable ID", path.display()));
    }

    if path.file_stem().and_then(|value| value.to_str()) != Some(record.id.as_str()) {
        return Err(format!(
            "{} filename does not match its stable ID",
            path.display()
        ));
    }

    for mode in &record.allowed_modes {
        if !["ammodino", "arranda"].contains(&mode.as_str()) {
            return Err(format!("{} has an invalid allowed mode", path.display()));
        }
    }

    let unique_modes = record.allowed_modes.iter().collect::<HashSet<_>>();

    if unique_modes.len() != record.allowed_modes.len() {
        return Err(format!("{} repeats an allowed mode", path.display()));
    }

    let mut unique_variants = HashSet::new();

    for variant in &record.variants {
        let normalized = variant.trim().to_lowercase();

        if normalized.is_empty()
            || normalized == record.canonical.trim().to_lowercase()
            || !unique_variants.insert(normalized)
        {
            return Err(format!("{} has an invalid variant", path.display()));
        }
    }

    let mut unique_examples = HashSet::new();

    for example in &record.examples {
        if example.livornese.trim().is_empty()
            || example.italian.trim().is_empty()
            || example.livornese.trim() == example.italian.trim()
            || !unique_examples.insert(example.livornese.trim().to_lowercase())
            || contains_wrong_de(&example.livornese)
        {
            return Err(format!("{} has an invalid example", path.display()));
        }
    }

    if !valid_date(&record.review.date) {
        return Err(format!("{} has an invalid review date", path.display()));
    }

    let mut has_local_attestation = false;

    for evidence in &record.evidence {
        if evidence.source_id.is_empty()
            || evidence.locator.is_empty()
            || !evidence.url.starts_with("https://")
            || !valid_date(&evidence.accessed)
            || evidence
                .archive_url
                .as_deref()
                .is_some_and(|url| !url.starts_with("https://"))
        {
            return Err(format!("{} has incomplete evidence", path.display()));
        }

        let source = source_metadata.get(&evidence.source_id).ok_or_else(|| {
            format!(
                "{} references unknown source {}",
                path.display(),
                evidence.source_id
            )
        })?;

        let archive_matches = match (&source.archive_url, &evidence.archive_url) {
            (Some(catalog), Some(evidence)) => source_url_matches(catalog, evidence),
            (None, None) => true,
            _ => false,
        };

        if evidence.locator.trim().len() < 8
            || !source_url_matches(&source.url, &evidence.url)
            || !archive_matches
        {
            return Err(format!(
                "{} evidence URLs do not match source {}",
                path.display(),
                evidence.source_id
            ));
        }

        has_local_attestation |= source.local_attestation;
    }

    if !has_local_attestation {
        return Err(format!(
            "{} lacks Livorno-specific attestation",
            path.display()
        ));
    }

    match record.runtime.as_str() {
        "spento" if record.runtime_priority != 0 => {
            return Err(format!(
                "{} non-runtime records need priority zero",
                path.display()
            ));
        }
        "ammodino" | "arranda"
            if record.runtime_priority == 0 || !record.allowed_modes.contains(&record.runtime) =>
        {
            return Err(format!(
                "{} runtime mode and priority are inconsistent",
                path.display()
            ));
        }
        _ => {}
    }

    Ok(())
}

fn validate_enum(path: &Path, field: &str, value: &str, allowed: &[&str]) -> Result<(), String> {
    if allowed.contains(&value) {
        Ok(())
    } else {
        Err(format!(
            "{} has invalid {field} value {value}",
            path.display()
        ))
    }
}

fn contains_wrong_de(value: &str) -> bool {
    value
        .split(|character: char| !character.is_alphabetic())
        .any(|word| word.eq_ignore_ascii_case("dè"))
}

fn source_url_matches(catalog_url: &str, evidence_url: &str) -> bool {
    evidence_url == catalog_url
        || (catalog_url.ends_with('/') && evidence_url.starts_with(catalog_url))
}

fn validate_record_relationships(records: &[Record]) -> Result<(), String> {
    let mut canonical_forms = HashMap::new();
    let mut aliases = HashMap::new();

    for record in records {
        let canonical = record.canonical.trim().to_lowercase();

        if let Some(previous) = canonical_forms.insert(canonical.clone(), record.id.as_str()) {
            return Err(format!(
                "canonical form {} is shared by {previous} and {}",
                record.canonical, record.id
            ));
        }

        for variant in &record.variants {
            let normalized = variant.trim().to_lowercase();

            if let Some(previous) = aliases.insert(normalized, record.id.as_str())
                && previous != record.id
            {
                return Err(format!(
                    "variant {variant} is shared by {previous} and {}",
                    record.id
                ));
            }
        }
    }

    for (alias, record_id) in &aliases {
        if let Some(canonical_id) = canonical_forms.get(alias)
            && canonical_id != record_id
        {
            return Err(format!(
                "variant {alias} from {record_id} collides with canonical form in {canonical_id}"
            ));
        }
    }

    let recognized_forms = canonical_forms
        .keys()
        .chain(aliases.keys())
        .map(String::as_str)
        .collect::<HashSet<_>>();

    for command in ["ammodino", "arranda", "spengi", "dé"] {
        if !recognized_forms.contains(command) {
            return Err(format!("accepted corpus lacks command form {command}"));
        }
    }

    Ok(())
}

fn validate_runtime_priorities(
    records: &[Record],
    mode: &str,
    expected_count: usize,
) -> Result<(), String> {
    let mut priorities = records
        .iter()
        .filter(|record| record.runtime == mode)
        .map(|record| record.runtime_priority as usize)
        .collect::<Vec<_>>();
    priorities.sort_unstable();
    let expected = (1..=expected_count).collect::<Vec<_>>();

    if priorities != expected {
        return Err(format!(
            "{mode} runtime priorities must be contiguous from 1 to {expected_count}"
        ));
    }

    Ok(())
}

fn valid_date(value: &str) -> bool {
    let bytes = value.as_bytes();

    if bytes.len() != 10
        || bytes[4] != b'-'
        || bytes[7] != b'-'
        || bytes
            .iter()
            .enumerate()
            .any(|(index, byte)| index != 4 && index != 7 && !byte.is_ascii_digit())
    {
        return false;
    }

    let year = u32::from(bytes[0] - b'0') * 1_000
        + u32::from(bytes[1] - b'0') * 100
        + u32::from(bytes[2] - b'0') * 10
        + u32::from(bytes[3] - b'0');
    let month = u32::from(bytes[5] - b'0') * 10 + u32::from(bytes[6] - b'0');
    let day = u32::from(bytes[8] - b'0') * 10 + u32::from(bytes[9] - b'0');
    let leap_year =
        year.is_multiple_of(4) && (!year.is_multiple_of(100) || year.is_multiple_of(400));
    let days = match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if leap_year => 29,
        2 => 28,
        _ => return false,
    };

    year >= 2000 && (1..=days).contains(&day)
}

fn sources_verify(option: Option<&str>) -> Result<(), String> {
    let metadata_only = option == Some("--metadata-only");

    if option.is_some() && !metadata_only {
        return Err("sources verify accepts only --metadata-only".to_owned());
    }

    let root = repo_root()?;

    let text = fs::read_to_string(root.join("corpus/sources.toml"))
        .map_err(|error| format!("read bibliography: {error}"))?;
    let sources: SourcesFile =
        toml::from_str(&text).map_err(|error| format!("parse bibliography: {error}"))?;

    validate_source_catalog(&sources)?;

    for source in &sources.source {
        if !metadata_only {
            println!("sources: checking live URL for {}", source.id);
            check_url(&source.url)?;

            if let Some(archive_url) = &source.archive_url {
                println!("sources: checking archive URL for {}", source.id);
                check_url(archive_url)?;
            } else {
                println!("sources: no verified archive URL for {}", source.id);
            }
        }
    }

    println!(
        "sources: {} bibliography entries {}",
        sources.source.len(),
        if metadata_only {
            "passed metadata verification"
        } else {
            "passed live/archive link verification"
        }
    );
    Ok(())
}

fn check_url(url: &str) -> Result<(), String> {
    let status = Command::new("curl")
        .args([
            "--fail",
            "--location",
            "--silent",
            "--show-error",
            "--max-time",
            "15",
            "--output",
            if cfg!(windows) { "NUL" } else { "/dev/null" },
            url,
        ])
        .stdout(Stdio::null())
        .status()
        .map_err(|error| format!("check {url}: cannot execute curl: {error}"))?;

    if !status.success() {
        return Err(format!("check {url}: curl exited with {status}"));
    }

    Ok(())
}

fn manifests_check() -> Result<(), String> {
    let root = repo_root()?;
    let version = fs::read_to_string(root.join("VERSION"))
        .map_err(|error| format!("read VERSION: {error}"))?;

    if version.trim() != VERSION {
        return Err("VERSION does not match the maintainer binary version".to_owned());
    }

    for (root_file, plugin_file) in [
        ("LICENSE", "plugins/toen/LICENSE"),
        ("CORPUS-LICENSE.md", "plugins/toen/CORPUS-LICENSE.md"),
    ] {
        let root_contents =
            fs::read(root.join(root_file)).map_err(|error| format!("read {root_file}: {error}"))?;
        let plugin_contents = fs::read(root.join(plugin_file))
            .map_err(|error| format!("read {plugin_file}: {error}"))?;

        if root_contents != plugin_contents {
            return Err(format!("{plugin_file} must match {root_file}"));
        }
    }

    let plugin = read_json(&root.join("plugins/toen/.codex-plugin/plugin.json"))?;
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

    let policy = fs::read_to_string(root.join("plugins/toen/skills/toen/agents/openai.yaml"))
        .map_err(|error| format!("read skill policy: {error}"))?;

    if !has_explicit_invocation_policy(&policy) {
        return Err("skill policy must disable implicit invocation".to_owned());
    }

    let marketplace_path = root.join(".agents/plugins/marketplace.json");
    let marketplace = read_json(&marketplace_path)?;
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

    if marketplace_name != Some("toen")
        || source_kind != Some("local")
        || path != Some("./plugins/toen")
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

    println!("manifests: plugin, skill policy, and marketplace passed validation");
    Ok(())
}

fn has_explicit_invocation_policy(yaml: &str) -> bool {
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

fn read_json(path: &Path) -> Result<Value, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;

    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn source_metadata(root: &Path) -> Result<HashMap<String, SourceMetadata>, String> {
    let text = fs::read_to_string(root.join("corpus/sources.toml"))
        .map_err(|error| format!("read bibliography: {error}"))?;
    let sources: SourcesFile =
        toml::from_str(&text).map_err(|error| format!("parse bibliography: {error}"))?;

    validate_source_catalog(&sources)?;

    Ok(sources
        .source
        .into_iter()
        .map(|source| {
            (
                source.id,
                SourceMetadata {
                    url: source.url,
                    archive_url: source.archive_url,
                    local_attestation: source.local_attestation,
                },
            )
        })
        .collect())
}

fn validate_source_catalog(sources: &SourcesFile) -> Result<(), String> {
    let mut ids = sources
        .source
        .iter()
        .map(|source| source.id.as_str())
        .collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();

    if ids.len() != sources.source.len() {
        return Err("bibliography source IDs must be unique".to_owned());
    }

    for source in &sources.source {
        if source.id.trim().is_empty()
            || source.name.trim().is_empty()
            || !source.url.starts_with("https://")
            || source
                .archive_url
                .as_deref()
                .is_some_and(|url| !url.starts_with("https://"))
        {
            return Err(format!("source {} has incomplete metadata", source.id));
        }
    }

    let local_count = sources
        .source
        .iter()
        .filter(|source| source.local_attestation)
        .count();

    if local_count == 0 {
        return Err("bibliography needs Livorno-specific sources".to_owned());
    }

    Ok(())
}

fn generate(check: bool) -> Result<(), String> {
    let root = repo_root()?;
    let records = load_records(&root)?;
    let grammar = load_grammar(&root)?;
    let skill = render_skill(&records, &grammar)?;
    let token_count = count_skill_tokens(&skill)?;

    if token_count > SKILL_TOKEN_BUDGET {
        return Err(format!(
            "generated skill uses {token_count} o200k_base tokens; budget is {SKILL_TOKEN_BUDGET}"
        ));
    }

    let assets = [
        ("plugins/toen/skills/toen/SKILL.md", skill.clone()),
        ("docs/generated-dictionary.md", render_dictionary(&records)),
        ("docs/source-notice.md", render_source_notice(&root)?),
        (
            "plugins/toen/SOURCE-NOTICE.md",
            render_source_notice(&root)?,
        ),
        (
            "docs/token-budget.json",
            render_budget_manifest(&skill, token_count)?,
        ),
    ];

    if check {
        for (relative_path, generated) in &assets {
            let path = root.join(relative_path);
            let current = fs::read_to_string(&path)
                .map_err(|error| format!("read {}: {error}", path.display()))?;

            if current != *generated {
                return Err(format!(
                    "{} is out of date; run `toenctl generate`",
                    path.display()
                ));
            }
        }

        println!("generate: 5 assets are up to date; skill uses {token_count}/750 tokens");
        return Ok(());
    }

    for (relative_path, contents) in assets {
        let path = root.join(relative_path);
        fs::write(&path, contents).map_err(|error| format!("write {}: {error}", path.display()))?;
        println!("generate: wrote {}", path.display());
    }

    println!("generate: skill uses {token_count}/750 tokens on both benchmark models");
    Ok(())
}

fn load_grammar(root: &Path) -> Result<Vec<GrammarRule>, String> {
    let path = root.join("corpus/grammar.toml");
    let text =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let grammar: GrammarFile =
        toml::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))?;
    let sources = source_metadata(root)?;

    if grammar.rule.len() != 12 {
        return Err(format!(
            "expected 12 grammar rules, found {}",
            grammar.rule.len()
        ));
    }

    for (index, rule) in grammar.rule.iter().enumerate() {
        let expected_id = format!("rule-{:02}", index + 1);

        if rule.id != expected_id
            || rule.text.trim().is_empty()
            || rule.modes.is_empty()
            || rule.source_ids.is_empty()
        {
            return Err(format!("invalid grammar rule {}", rule.id));
        }

        if rule
            .modes
            .iter()
            .any(|mode| !["ammodino", "arranda"].contains(&mode.as_str()))
            || rule.modes.iter().collect::<HashSet<_>>().len() != rule.modes.len()
        {
            return Err(format!("{} has an invalid mode", rule.id));
        }

        if rule
            .source_ids
            .iter()
            .any(|source_id| !sources.contains_key(source_id))
            || rule.source_ids.iter().collect::<HashSet<_>>().len() != rule.source_ids.len()
            || !rule
                .source_ids
                .iter()
                .any(|source_id| sources[source_id].local_attestation)
        {
            return Err(format!(
                "{} needs unique, known, Livorno-specific source IDs",
                rule.id
            ));
        }
    }

    Ok(grammar.rule)
}

fn render_skill(records: &[Record], grammar: &[GrammarRule]) -> Result<String, String> {
    let mut ammodino = records
        .iter()
        .filter(|record| record.runtime == "ammodino")
        .collect::<Vec<_>>();
    let mut arranda = records
        .iter()
        .filter(|record| record.runtime == "arranda")
        .collect::<Vec<_>>();

    ammodino.sort_by_key(|record| record.runtime_priority);
    arranda.sort_by_key(|record| record.runtime_priority);

    if ammodino.len() != AMMODINO_COUNT || arranda.len() != ARRANDA_COUNT {
        return Err("runtime core does not contain the required 50/30 records".to_owned());
    }

    let ammodino_forms = ammodino
        .iter()
        .map(|record| record.canonical.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let arranda_forms = arranda
        .iter()
        .map(|record| record.canonical.as_str())
        .collect::<Vec<_>>()
        .join(", ");
    let rules = grammar
        .iter()
        .enumerate()
        .map(|(index, rule)| format!("{}. {}", index + 1, rule.text))
        .collect::<Vec<_>>()
        .join("\n");

    Ok(format!(
        r#"---
name: toen
description: Explicit contemporary Livornese for concise assistant replies.
---

# Toen

Default `spento`; only `$toen` activates. New sessions reset; resume/compaction retain mode.

Commands: `$toen` chooses in user's language; `ammodino|arranda [task]` activates, optionally running task; `de` reports; `spengi [task]` deactivates, optionally running task. Unknown: usage without state change.

Apply only to visible replies/status/tool narration. Keep requested deliverable language; technical terms standard. Preserve code, commands, paths, URLs, IDs, logs, errors, quotes, and numbers exactly. No slur guidance or hidden-reasoning claims. Detail and host safety win.

Rules:
{rules}

Ammodino core: {ammodino_forms}.

Arranda adds: {arranda_forms}.
"#
    ))
}

fn render_dictionary(records: &[Record]) -> String {
    let mut output = String::from(
        "# Generated Dictionary\n\nGenerated from the accepted TOML corpus. Runtime records are marked Ammodino or Arranda; non-runtime records are marked Spento.\n\n| ID | Form | Italian | English | Runtime | Source |\n| --- | --- | --- | --- | --- | --- |\n",
    );

    for record in records {
        let sources = record
            .evidence
            .iter()
            .map(|evidence| evidence.source_id.as_str())
            .collect::<Vec<_>>()
            .join(", ");

        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            escape_table(&record.id),
            escape_table(&record.canonical),
            escape_table(&record.gloss_it),
            escape_table(&record.gloss_en),
            escape_table(&record.runtime),
            escape_table(&sources),
        ));
    }

    output
}

fn escape_table(value: &str) -> String {
    value.replace('|', "\\|").replace('\n', " ")
}

fn render_source_notice(root: &Path) -> Result<String, String> {
    let text = fs::read_to_string(root.join("corpus/sources.toml"))
        .map_err(|error| format!("read bibliography: {error}"))?;
    let sources: SourcesFile =
        toml::from_str(&text).map_err(|error| format!("parse bibliography: {error}"))?;
    let mut output = String::from(
        "# Source Notice\n\nToen ships original examples and distilled notes, not copied source pages. Evidence locators and access dates remain in each corpus record.\n\n",
    );

    for source in sources.source {
        let role = if source.local_attestation {
            "Livorno-specific attestation"
        } else {
            "supporting source"
        };

        output.push_str(&format!(
            "- **{}** (`{}`, {role}): {}\n",
            source.name, source.id, source.url
        ));
    }

    Ok(output)
}

fn count_skill_tokens(skill: &str) -> Result<usize, String> {
    let tokenizer = o200k_base().map_err(|error| format!("load o200k_base tokenizer: {error}"))?;
    Ok(tokenizer.encode_with_special_tokens(skill).len())
}

fn render_budget_manifest(skill: &str, token_count: usize) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(skill.as_bytes());

    serde_json::to_string_pretty(&serde_json::json!({
        "version": VERSION,
        "skill_token_budget": SKILL_TOKEN_BUDGET,
        "encoding": "o200k_base",
        "models": {
            "gpt-5.6-luna": token_count,
            "gpt-5.6-sol": token_count
        },
        "runtime_core": {
            "ammodino": AMMODINO_COUNT,
            "arranda": ARRANDA_COUNT
        },
        "skill_sha256": format!("{:x}", hasher.finalize())
    }))
    .map(|json| format!("{json}\n"))
    .map_err(|error| format!("serialize token budget: {error}"))
}

fn package(args: &[String]) -> Result<(), String> {
    let [flag, version] = args else {
        return Err("package requires --version <version>".to_owned());
    };

    if flag != "--version" {
        return Err("package requires --version <version>".to_owned());
    }

    if version.as_str() != VERSION {
        return Err(format!(
            "package version {version} does not match repository version {VERSION}"
        ));
    }

    let root = repo_root()?;
    corpus_check()?;
    manifests_check()?;
    generate(true)?;
    bench::release_gates_pass(&root, version)?;

    let dist = root.join("dist");
    fs::create_dir_all(&dist).map_err(|error| format!("create dist: {error}"))?;

    let plugin_archive = dist.join(format!("toen-plugin-v{version}.zip"));
    let skill_archive = dist.join(format!("toen-skill-v{version}.zip"));
    let evidence_archive = dist.join(format!("toen-benchmark-evidence-v{version}.zip"));
    let benchmark_report = dist.join(format!("toen-benchmark-report-v{version}.md"));

    write_zip(&plugin_archive, &root, "plugins/toen")?;
    write_skill_zip(&skill_archive, &root)?;
    write_benchmark_zip(&evidence_archive, &root, version)?;
    fs::copy(
        root.join("benchmarks/releases")
            .join(version)
            .join("report.md"),
        &benchmark_report,
    )
    .map_err(|error| format!("copy benchmark report: {error}"))?;

    let checksums = [
        plugin_archive,
        skill_archive,
        evidence_archive,
        benchmark_report,
    ]
    .iter()
    .map(|path| sha256_line(path))
    .collect::<Result<Vec<_>, _>>()?
    .join("");

    fs::write(
        dist.join(format!("toen-v{version}-checksums.txt")),
        checksums,
    )
    .map_err(|error| format!("write checksum: {error}"))?;

    println!(
        "package: wrote plugin, raw skill, benchmark report, evidence archive, and checksums in dist/"
    );
    Ok(())
}

fn write_benchmark_zip(destination: &Path, root: &Path, version: &str) -> Result<(), String> {
    let file = fs::File::create(destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let release = root.join("benchmarks/releases").join(version);

    add_directory_to_zip_filtered(&mut zip, options, &release, "release", &["work"])?;

    for path in [
        "benchmarks/scenarios.json",
        "benchmarks/sessions.json",
        "benchmarks/rubric.md",
        "benchmarks/judge.schema.json",
        "benchmarks/compatibility.schema.json",
    ] {
        add_file_to_zip(&mut zip, options, path, &root.join(path))?;
    }

    add_directory_to_zip(
        &mut zip,
        options,
        &root.join("benchmarks/fixtures"),
        "benchmarks/fixtures",
    )?;
    add_file_to_zip(
        &mut zip,
        options,
        "CORPUS-LICENSE.md",
        &root.join("CORPUS-LICENSE.md"),
    )?;

    zip.finish()
        .map_err(|error| format!("finish {}: {error}", destination.display()))?;
    Ok(())
}

fn write_zip(destination: &Path, root: &Path, relative_dir: &str) -> Result<(), String> {
    let file = fs::File::create(destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let base = root.join(relative_dir);

    add_directory_to_zip(&mut zip, options, &base, "")?;
    zip.finish()
        .map_err(|error| format!("finish {}: {error}", destination.display()))?;
    Ok(())
}

fn write_skill_zip(destination: &Path, root: &Path) -> Result<(), String> {
    let file = fs::File::create(destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let skill = root.join("plugins/toen/skills/toen/SKILL.md");
    let contents = fs::read(&skill).map_err(|error| format!("read skill: {error}"))?;

    zip.start_file("SKILL.md", options)
        .map_err(|error| format!("start skill archive entry: {error}"))?;

    std::io::Write::write_all(&mut zip, &contents)
        .map_err(|error| format!("write skill archive: {error}"))?;

    add_file_to_zip(
        &mut zip,
        options,
        "agents/openai.yaml",
        &root.join("plugins/toen/skills/toen/agents/openai.yaml"),
    )?;
    add_file_to_zip(&mut zip, options, "LICENSE", &root.join("LICENSE"))?;
    add_file_to_zip(
        &mut zip,
        options,
        "CORPUS-LICENSE.md",
        &root.join("CORPUS-LICENSE.md"),
    )?;
    add_file_to_zip(
        &mut zip,
        options,
        "SOURCE-NOTICE.md",
        &root.join("docs/source-notice.md"),
    )?;

    zip.finish()
        .map_err(|error| format!("finish skill archive: {error}"))?;
    Ok(())
}

fn add_file_to_zip(
    zip: &mut ZipWriter<fs::File>,
    options: FileOptions,
    archive_path: &str,
    path: &Path,
) -> Result<(), String> {
    let contents = fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?;

    zip.start_file(archive_path, options)
        .map_err(|error| format!("start package entry: {error}"))?;

    std::io::Write::write_all(zip, &contents)
        .map_err(|error| format!("write package entry: {error}"))?;
    Ok(())
}

fn add_directory_to_zip(
    zip: &mut ZipWriter<fs::File>,
    options: FileOptions,
    base: &Path,
    archive_prefix: &str,
) -> Result<(), String> {
    let mut entries = fs::read_dir(base)
        .map_err(|error| format!("read package directory {}: {error}", base.display()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("read package entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();

    for path in entries {
        let name = path
            .file_name()
            .ok_or_else(|| format!("package path has no name: {}", path.display()))?
            .to_string_lossy();
        let archive_path = if archive_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{archive_prefix}/{name}")
        };

        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("inspect package path {}: {error}", path.display()))?;

        if metadata.file_type().is_symlink() {
            return Err(format!(
                "package input must not contain symlinks: {}",
                path.display()
            ));
        }

        if metadata.is_dir() {
            add_directory_to_zip(zip, options, &path, &archive_path)?;
        } else {
            let contents = fs::read(&path)
                .map_err(|error| format!("read package file {}: {error}", path.display()))?;

            zip.start_file(archive_path, options)
                .map_err(|error| format!("start package entry: {error}"))?;

            std::io::Write::write_all(zip, &contents)
                .map_err(|error| format!("write package entry: {error}"))?;
        }
    }

    Ok(())
}

fn add_directory_to_zip_filtered(
    zip: &mut ZipWriter<fs::File>,
    options: FileOptions,
    base: &Path,
    archive_prefix: &str,
    excluded_names: &[&str],
) -> Result<(), String> {
    let mut entries = fs::read_dir(base)
        .map_err(|error| format!("read package directory {}: {error}", base.display()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("read package entry: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    entries.sort();

    for path in entries {
        let name = path
            .file_name()
            .ok_or_else(|| format!("package path has no name: {}", path.display()))?
            .to_string_lossy();

        if excluded_names.contains(&name.as_ref()) {
            continue;
        }

        let archive_path = if archive_prefix.is_empty() {
            name.to_string()
        } else {
            format!("{archive_prefix}/{name}")
        };
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("inspect package path {}: {error}", path.display()))?;

        if metadata.file_type().is_symlink() {
            return Err(format!(
                "package input must not contain symlinks: {}",
                path.display()
            ));
        }

        if metadata.is_dir() {
            add_directory_to_zip_filtered(zip, options, &path, &archive_path, excluded_names)?;
        } else {
            add_file_to_zip(zip, options, &archive_path, &path)?;
        }
    }

    Ok(())
}

fn sha256_line(path: &Path) -> Result<String, String> {
    let mut hasher = Sha256::new();
    hasher.update(fs::read(path).map_err(|error| format!("read {}: {error}", path.display()))?);
    let filename = path
        .file_name()
        .ok_or_else(|| format!("checksum path has no file name: {}", path.display()))?;

    Ok(format!(
        "{:x}  {}\n",
        hasher.finalize(),
        filename.to_string_lossy()
    ))
}

#[cfg(test)]
fn parse_command(input: &str) -> ToenCommand {
    let mut words = input.split_whitespace();

    if words.next() != Some("$toen") {
        return ToenCommand::Usage;
    }

    match words.next() {
        None => ToenCommand::Chooser,
        Some("ammodino") | Some("arranda") => {
            let mode = input.split_whitespace().nth(1).unwrap().to_owned();
            let task = words.collect::<Vec<_>>().join(" ");

            if task.is_empty() {
                ToenCommand::Activate(mode)
            } else {
                ToenCommand::ActivateAndTask(mode, task)
            }
        }
        Some("de") => ToenCommand::Status,
        Some("spengi") => {
            let task = words.collect::<Vec<_>>().join(" ");

            if task.is_empty() {
                ToenCommand::Deactivate
            } else {
                ToenCommand::DeactivateAndTask(task)
            }
        }
        Some(_) => ToenCommand::Usage,
    }
}

fn repo_root() -> Result<PathBuf, String> {
    let current = env::current_dir().map_err(|error| format!("current directory: {error}"))?;
    let root = if current.join("corpus/accepted").is_dir() {
        current
    } else if current.join("../corpus/accepted").is_dir() {
        current.join("..")
    } else {
        return Err("run toenctl from the repository root or its toenctl directory".to_owned());
    };

    Ok(root)
}

#[cfg(test)]
mod tests {
    use std::io::Read;

    use super::*;
    use zip::ZipArchive;

    #[test]
    fn command_words_are_stable() {
        assert!(
            "ammodino arranda de spengi"
                .split_whitespace()
                .all(|word| !word.is_empty())
        );
    }

    #[test]
    fn budget_manifest_is_deterministic() {
        let manifest = render_budget_manifest("skill", 1).unwrap();

        assert!(manifest.contains("750"));
        assert!(manifest.contains("ammodino"));
        assert!(manifest.contains("o200k_base"));
    }

    #[test]
    fn dates_are_calendar_valid_and_never_panic_on_unicode() {
        assert!(valid_date("2024-02-29"));
        assert!(!valid_date("2023-02-29"));
        assert!(!valid_date("2026-13-01"));
        assert!(!valid_date("2026-00-01"));
        assert!(!valid_date("2026-01-00"));
        assert!(!valid_date("é026-01-01"));
        assert!(!valid_date("2026-0é-01"));
    }

    #[test]
    fn command_parser_preserves_the_stable_contract() {
        assert_eq!(parse_command("$toen"), ToenCommand::Chooser);
        assert_eq!(
            parse_command("$toen ammodino"),
            ToenCommand::Activate("ammodino".to_owned())
        );
        assert_eq!(
            parse_command("$toen arranda fix this"),
            ToenCommand::ActivateAndTask("arranda".to_owned(), "fix this".to_owned())
        );
        assert_eq!(parse_command("$toen de"), ToenCommand::Status);
        assert_eq!(parse_command("$toen spengi"), ToenCommand::Deactivate);
        assert_eq!(
            parse_command("$toen spengi explain this"),
            ToenCommand::DeactivateAndTask("explain this".to_owned())
        );
        assert_eq!(parse_command("$toen nope"), ToenCommand::Usage);
    }

    #[test]
    fn raw_skill_archives_are_reproducible_and_keep_invocation_policy() {
        let root = repo_root().unwrap();
        let first = std::env::temp_dir().join(format!(
            "toen-skill-policy-first-{}.zip",
            std::process::id()
        ));
        let second = std::env::temp_dir().join(format!(
            "toen-skill-policy-second-{}.zip",
            std::process::id()
        ));

        write_skill_zip(&first, &root).unwrap();
        write_skill_zip(&second, &root).unwrap();

        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());

        let archive = fs::File::open(&first).unwrap();
        let mut archive = ZipArchive::new(archive).unwrap();
        let mut policy = archive.by_name("agents/openai.yaml").unwrap();
        let mut contents = String::new();
        policy.read_to_string(&mut contents).unwrap();

        assert!(contents.contains("allow_implicit_invocation: false"));

        drop(policy);
        assert!(archive.by_name("LICENSE").is_ok());
        assert!(archive.by_name("CORPUS-LICENSE.md").is_ok());
        assert!(archive.by_name("SOURCE-NOTICE.md").is_ok());

        drop(archive);
        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
    }

    #[test]
    fn plugin_archives_are_reproducible_and_complete() {
        let root = repo_root().unwrap();
        let first = std::env::temp_dir().join(format!(
            "toen-plugin-repro-first-{}.zip",
            std::process::id()
        ));
        let second = std::env::temp_dir().join(format!(
            "toen-plugin-repro-second-{}.zip",
            std::process::id()
        ));

        write_zip(&first, &root, "plugins/toen").unwrap();
        write_zip(&second, &root, "plugins/toen").unwrap();

        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());

        let archive = fs::File::open(&first).unwrap();
        let mut archive = ZipArchive::new(archive).unwrap();

        assert!(archive.by_name(".codex-plugin/plugin.json").is_ok());
        assert!(archive.by_name("skills/toen/SKILL.md").is_ok());
        assert!(archive.by_name("skills/toen/agents/openai.yaml").is_ok());
        assert!(archive.by_name("LICENSE").is_ok());
        assert!(archive.by_name("CORPUS-LICENSE.md").is_ok());
        assert!(archive.by_name("SOURCE-NOTICE.md").is_ok());

        drop(archive);
        fs::remove_file(first).unwrap();
        fs::remove_file(second).unwrap();
    }
}
