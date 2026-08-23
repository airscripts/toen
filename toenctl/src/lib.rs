#![forbid(unsafe_code)]
#![deny(rustdoc::broken_intra_doc_links)]

mod bench;
mod cli;
mod config;
mod error;
mod manifests;
mod packaging;
mod sources;
pub(crate) mod toenizer;
pub(crate) mod workspace;

use std::collections::{HashMap, HashSet};
use std::env;
use std::fs;
use std::path::Path;
#[cfg(test)]
use std::path::PathBuf;
use std::process::Command;

use schemars::{
    JsonSchema,
    r#gen::SchemaGenerator,
    schema::{Schema, StringValidation},
};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tiktoken_rs::o200k_base;

pub use error::ToenError;

pub(crate) const VERSION: &str = env!("CARGO_PKG_VERSION");

macro_rules! record_enum {
    ($name:ident { $( $variant:ident => $value:literal ),+ $(,)? }) => {
        #[derive(Clone, Copy, Debug, Deserialize, Eq, Hash, JsonSchema, PartialEq, Serialize)]
        #[serde(rename_all = "lowercase")]
        enum $name {
            $(#[serde(rename = $value)] $variant),+
        }

        impl $name {
            fn as_str(self) -> &'static str {
                match self { $(Self::$variant => $value),+ }
            }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str(self.as_str())
            }
        }
    };
}

record_enum!(RecordKind {
    Lexeme => "lexeme",
    Idiom => "idiom",
    Particle => "particle",
});
record_enum!(Register {
    Everyday => "everyday",
    Colloquial => "colloquial",
    Expressive => "expressive",
});
record_enum!(GrammaticalRole {
    Adjective => "adjective",
    Adverb => "adverb",
    Discourse => "discourse",
    Interjection => "interjection",
    Noun => "noun",
    Phrase => "phrase",
    Pronoun => "pronoun",
    Verb => "verb",
});
record_enum!(ContemporaryStatus {
    Current => "current",
    Endangered => "endangered",
});
record_enum!(Confidence {
    High => "high",
    Medium => "medium",
});
record_enum!(RuntimeMode {
    Spento => "spento",
    Ammodino => "ammodino",
    Arranda => "arranda",
});

#[derive(Clone, Debug, Deserialize, Serialize)]
struct HttpsUrl(String);

impl JsonSchema for HttpsUrl {
    fn schema_name() -> String {
        "HttpsUrl".to_owned()
    }

    fn json_schema(generator: &mut SchemaGenerator) -> Schema {
        let mut schema = <String as JsonSchema>::json_schema(generator);
        if let Schema::Object(object) = &mut schema {
            object.format = Some("uri".to_owned());
            object.string = Some(Box::new(StringValidation {
                pattern: Some("^https://".to_owned()),
                ..StringValidation::default()
            }));
        }
        schema
    }
}

impl HttpsUrl {
    fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for HttpsUrl {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.write_str(self.as_str())
    }
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct Record {
    id: String,
    canonical: String,
    lemma: String,
    kind: RecordKind,
    gloss_it: String,
    gloss_en: String,
    register: Register,
    grammatical_role: GrammaticalRole,
    contemporary_status: ContemporaryStatus,
    confidence: Confidence,
    variants: Vec<String>,
    usage_notes: String,
    allowed_modes: Vec<RuntimeMode>,
    runtime: RuntimeMode,
    runtime_priority: u32,
    examples: Vec<Example>,
    evidence: Vec<Evidence>,
    review: Review,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct Example {
    livornese: String,
    italian: String,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct Evidence {
    source_id: String,
    locator: String,
    #[schemars(url)]
    url: HttpsUrl,
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    accessed: String,
    #[schemars(url)]
    archive_url: Option<HttpsUrl>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct Review {
    reviewer: String,
    #[schemars(regex(pattern = r"^\d{4}-\d{2}-\d{2}$"))]
    date: String,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct Source {
    id: String,
    name: String,
    #[schemars(url)]
    url: HttpsUrl,
    #[schemars(url)]
    archive_url: Option<HttpsUrl>,
    local_attestation: bool,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct SourcesFile {
    source: Vec<Source>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GrammarFile {
    rule: Vec<GrammarRule>,
}

#[derive(Debug, Deserialize, JsonSchema)]
struct GrammarRule {
    id: String,
    modes: Vec<RuntimeMode>,
    text: String,
    source_ids: Vec<String>,
}

#[derive(Debug)]
struct SourceMetadata {
    url: String,
    archive_url: Option<String>,
    local_attestation: bool,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub(crate) struct ProjectConfigSchema {
    pub(crate) schema_version: u32,
    pub(crate) accepted_records: usize,
    pub(crate) runtime_core: RuntimeCoreSchema,
    pub(crate) tokenizer: TokenizerSchema,
    pub(crate) budgets: BudgetsSchema,
    pub(crate) generated: GeneratedSchema,
    pub(crate) integrations: IntegrationsSchema,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub(crate) struct BudgetSchema {
    pub(crate) tokens: usize,
    pub(crate) utf8_bytes: usize,
    pub(crate) lines: usize,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub(crate) struct BudgetsSchema {
    pub(crate) portable: BudgetSchema,
    pub(crate) codex: BudgetSchema,
    #[serde(rename = "claude-code")]
    pub(crate) claude_code: BudgetSchema,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub(crate) struct RuntimeCoreSchema {
    pub(crate) ammodino: usize,
    pub(crate) arranda: usize,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub(crate) struct TokenizerSchema {
    pub(crate) id: String,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub(crate) struct GeneratedSchema {
    pub(crate) assets: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
pub(crate) struct IntegrationsSchema {
    pub(crate) supported: Vec<String>,
}

#[derive(Debug, Deserialize, JsonSchema, Serialize)]
struct PackageManifestSchema {
    name: String,
    version: String,
    description: String,
    skills: String,
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

pub fn run(args: Vec<String>) -> Result<(), ToenError> {
    cli::run(args)
}

pub(crate) fn verify(root: &Path) -> Result<(), String> {
    project_config_check(root)?;
    run_command(root, "cargo", &["fmt", "--check"])?;
    run_command(
        root,
        "cargo",
        &[
            "clippy",
            "--workspace",
            "--all-targets",
            "--locked",
            "--",
            "-D",
            "warnings",
        ],
    )?;
    run_command(root, "cargo", &["check", "--workspace", "--locked"])?;
    corpus_check(root)?;
    sources::verify(root, Some("--metadata-only"))?;
    manifests::check(root)?;
    generate(root, true)?;
    bench::run(root, &["smoke".to_owned(), "--check".to_owned()], VERSION)?;
    println!(
        "verify: formatting, lint, compilation, corpus, sources, manifests, generation, benchmark manifest, and Toenizer passed"
    );
    Ok(())
}

fn project_config_check(root: &Path) -> Result<(), String> {
    config::load(root).map(|_| ())
}

pub(crate) fn test(root: &Path) -> Result<(), String> {
    run_command(
        root,
        "cargo",
        &["test", "--workspace", "--all-targets", "--locked"],
    )?;
    run_command(
        root,
        "cargo",
        &[
            "llvm-cov",
            "--workspace",
            "--all-targets",
            "--locked",
            "--fail-under-lines",
            "81",
        ],
    )?;
    println!("test: workspace tests and 81% line coverage passed");
    Ok(())
}

pub(crate) fn doctor(root: &Path) -> Result<(), String> {
    println!("Workspace: {}", root.display());
    println!("OS: {}", env::consts::OS);
    println!("Architecture: {}", env::consts::ARCH);
    println!("Rust: {}", rust_version());
    println!("Coverage Runner: {}", command_available("cargo-llvm-cov"));
    println!("Container Engine: {}", optional_command_available("docker"));
    println!("Codex CLI: {}", optional_command_available("codex"));
    println!("Claude Code CLI: {}", optional_command_available("claude"));
    println!(
        "Network Verification: available when `sources verify` is run without --metadata-only"
    );
    println!("Generated Drift: run `toenctl generate --check`");
    Ok(())
}

fn run_command(root: &Path, program: &str, args: &[&str]) -> Result<(), String> {
    let status = Command::new(program)
        .args(args)
        .current_dir(root)
        .env("CARGO_BUILD_JOBS", "4")
        .status()
        .map_err(|error| format!("run {program}: {error}"))?;
    if status.success() {
        Ok(())
    } else {
        Err(format!("{program} {} exited with {status}", args.join(" ")))
    }
}

fn rust_version() -> String {
    Command::new("rustc")
        .arg("--version")
        .output()
        .ok()
        .filter(|output| output.status.success())
        .map(|output| String::from_utf8_lossy(&output.stdout).trim().to_owned())
        .unwrap_or_else(|| "unavailable".to_owned())
}

fn command_available(command: &str) -> bool {
    Command::new(command)
        .arg("--version")
        .output()
        .is_ok_and(|output| output.status.success())
}

fn optional_command_available(command: &str) -> &'static str {
    if command_available(command) {
        "available"
    } else {
        "not found (optional)"
    }
}

pub(crate) fn corpus_check(root: &Path) -> Result<(), String> {
    let config = config::load(root)?;
    let records = load_records(root)?;

    if records.len() != config.accepted_records {
        return Err(format!(
            "expected {} accepted records, found {}",
            config.accepted_records,
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
        .filter(|record| record.runtime == RuntimeMode::Ammodino)
        .count();
    let arranda = records
        .iter()
        .filter(|record| record.runtime == RuntimeMode::Arranda)
        .count();

    if ammodino != config.runtime_core.ammodino || arranda != config.runtime_core.arranda {
        return Err(format!(
            "expected runtime core {}/{}, found {ammodino}/{arranda}",
            config.runtime_core.ammodino, config.runtime_core.arranda
        ));
    }

    validate_runtime_priorities(
        &records,
        RuntimeMode::Ammodino,
        config.runtime_core.ammodino,
    )?;
    validate_runtime_priorities(&records, RuntimeMode::Arranda, config.runtime_core.arranda)?;

    load_grammar(root)?;

    println!(
        "corpus: {} accepted records; runtime core {ammodino} ammodino + {arranda} arranda",
        config.accepted_records
    );
    Ok(())
}

fn load_records(root: &Path) -> Result<Vec<Record>, String> {
    let records_dir = root.join("corpus/accepted");
    let source_metadata = sources::metadata(root)?;
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

    if record.contemporary_status == ContemporaryStatus::Current
        && record.confidence != Confidence::High
    {
        return Err(format!(
            "{} current records must have high confidence",
            path.display()
        ));
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
        if *mode == RuntimeMode::Spento {
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
            || !evidence.url.as_str().starts_with("https://")
            || !valid_date(&evidence.accessed)
            || evidence
                .archive_url
                .as_ref()
                .is_some_and(|url| !url.as_str().starts_with("https://"))
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
            (Some(catalog), Some(evidence)) => {
                source_url_matches(catalog.as_str(), evidence.as_str())
            }
            (None, None) => true,
            _ => false,
        };

        if evidence.locator.trim().len() < 8
            || !source_url_matches(source.url.as_str(), evidence.url.as_str())
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

    match record.runtime {
        RuntimeMode::Spento if record.runtime_priority != 0 => {
            return Err(format!(
                "{} non-runtime records need priority zero",
                path.display()
            ));
        }
        RuntimeMode::Ammodino | RuntimeMode::Arranda
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

#[cfg(test)]
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
    mode: RuntimeMode,
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

fn read_json(path: &Path) -> Result<Value, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;

    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

pub(crate) fn generate(root: &Path, check: bool) -> Result<(), String> {
    let config = config::load(root)?;
    let records = load_records(root)?;
    let grammar = load_grammar(root)?;
    let portable_skill = render_skill(
        &records,
        &grammar,
        config.runtime_core.ammodino,
        config.runtime_core.arranda,
    )?;
    let codex_skill = portable_skill.clone();
    let claude_skill = render_claude_skill(
        &records,
        &grammar,
        config.runtime_core.ammodino,
        config.runtime_core.arranda,
    )?;
    let token_count = count_skill_tokens(&portable_skill)?;
    let claude_token_count = count_skill_tokens(&claude_skill)?;

    validate_skill_budget(
        "portable",
        &portable_skill,
        token_count,
        config.budgets.portable.tokens,
        config.budgets.portable.utf8_bytes,
        config.budgets.portable.lines,
    )?;
    validate_skill_budget(
        "Codex",
        &codex_skill,
        token_count,
        config.budgets.codex.tokens,
        config.budgets.codex.utf8_bytes,
        config.budgets.codex.lines,
    )?;
    validate_skill_budget(
        "Claude Code",
        &claude_skill,
        claude_token_count,
        config.budgets.claude_code.tokens,
        config.budgets.claude_code.utf8_bytes,
        config.budgets.claude_code.lines,
    )?;

    let source_notice = render_source_notice(root)?;
    let assets = vec![
        ("skill/toen/SKILL.md", portable_skill.clone()),
        (
            "plugins/codex/toen/skills/toen/SKILL.md",
            codex_skill.clone(),
        ),
        (
            "plugins/claude-code/toen/skills/toen/SKILL.md",
            claude_skill.clone(),
        ),
        ("docs/generated-dictionary.md", render_dictionary(&records)),
        ("docs/source-notice.md", source_notice.clone()),
        ("skill/toen/SOURCE-NOTICE.md", source_notice.clone()),
        ("plugins/codex/toen/SOURCE-NOTICE.md", source_notice.clone()),
        ("plugins/claude-code/toen/SOURCE-NOTICE.md", source_notice),
        (
            "docs/token-budget.json",
            render_budget_manifest(&config, &portable_skill, &codex_skill, &claude_skill)?,
        ),
        ("schemas/corpus-record.schema.json", render_schema_record()?),
        ("schemas/sources.schema.json", render_schema_sources()?),
        ("schemas/grammar.schema.json", render_schema_grammar()?),
        (
            "schemas/project-config.schema.json",
            render_schema_config()?,
        ),
        (
            "schemas/package-manifest.schema.json",
            render_schema_package_manifest()?,
        ),
        ("schemas/toenizer.schema.json", toenizer::render_schema()?),
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

        println!(
            "generate: {} assets are up to date; portable skill uses {token_count}/{} tokens",
            assets.len(),
            config.budgets.portable.tokens
        );
        let report = toenizer::build_report(root)?;
        toenizer::check_report(root, &report)?;
        return Ok(());
    }

    for (relative_path, contents) in assets {
        let path = root.join(relative_path);
        atomic_write(&path, contents.as_bytes())?;
        println!("generate: wrote {}", path.display());
    }

    let report = toenizer::build_report(root)?;
    toenizer::write_report(root, &report)?;
    println!(
        "generate: skill uses {token_count}/{} tokens; Claude Code uses {claude_token_count}/{}",
        config.budgets.portable.tokens, config.budgets.claude_code.tokens
    );
    Ok(())
}

fn validate_skill_budget(
    label: &str,
    skill: &str,
    token_count: usize,
    token_budget: usize,
    byte_budget: usize,
    line_budget: usize,
) -> Result<(), String> {
    let bytes = skill.len();
    let lines = if skill.is_empty() {
        0
    } else {
        skill.lines().count()
    };
    if token_count > token_budget || bytes > byte_budget || lines > line_budget {
        return Err(format!(
            "generated {label} skill exceeds budget: {token_count}/{token_budget} tokens, {bytes}/{byte_budget} bytes, {lines}/{line_budget} lines"
        ));
    }
    Ok(())
}

fn load_grammar(root: &Path) -> Result<Vec<GrammarRule>, String> {
    let path = root.join("corpus/grammar.toml");
    let text =
        fs::read_to_string(&path).map_err(|error| format!("read {}: {error}", path.display()))?;
    let grammar: GrammarFile =
        toml::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))?;
    let sources = sources::metadata(root)?;

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

        if rule.modes.contains(&RuntimeMode::Spento)
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

fn render_skill(
    records: &[Record],
    grammar: &[GrammarRule],
    ammodino_count: usize,
    arranda_count: usize,
) -> Result<String, String> {
    let body = render_skill_body(records, grammar, ammodino_count, arranda_count)?;

    Ok(format!(
        "---\nname: toen\ndescription: Explicit contemporary Livornese for concise assistant replies.\n---\n\n<!-- Generated File—Do Not Edit. -->\n\n{body}"
    ))
}

fn render_claude_skill(
    records: &[Record],
    grammar: &[GrammarRule],
    ammodino_count: usize,
    arranda_count: usize,
) -> Result<String, String> {
    let body = render_skill_body(records, grammar, ammodino_count, arranda_count)?;

    Ok(format!(
        "---\nname: toen\ndescription: Explicit contemporary Livornese for concise assistant replies.\ndisable-model-invocation: true\nargument-hint: \"[ammodino|arranda|de|spengi] [task]\"\n---\n\n<!-- Generated File—Do Not Edit. -->\n\nInvoke explicitly as `/toen:toen [command] [task]`; it maps to the canonical `$toen` protocol.\n\n{body}"
    ))
}

fn render_skill_body(
    records: &[Record],
    grammar: &[GrammarRule],
    ammodino_count: usize,
    arranda_count: usize,
) -> Result<String, String> {
    let mut ammodino = records
        .iter()
        .filter(|record| record.runtime == RuntimeMode::Ammodino)
        .collect::<Vec<_>>();
    let mut arranda = records
        .iter()
        .filter(|record| record.runtime == RuntimeMode::Arranda)
        .collect::<Vec<_>>();

    ammodino.sort_by_key(|record| record.runtime_priority);
    arranda.sort_by_key(|record| record.runtime_priority);

    if ammodino.len() != ammodino_count || arranda.len() != arranda_count {
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
        r#"# Toen

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
        "<!-- Generated File—Do Not Edit. -->\n# Generated Dictionary\n\nGenerated from the accepted TOML corpus. Runtime records are marked Ammodino or Arranda; non-runtime records are marked Spento.\n\n| ID | Form | Italian | English | Runtime | Source |\n| --- | --- | --- | --- | --- | --- |\n",
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
            escape_table(record.runtime.as_str()),
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
        "<!-- Generated File—Do Not Edit. -->\n# Source Notice\n\nToen ships original examples and distilled notes, not copied source pages. Evidence locators and access dates remain in each corpus record.\n\n",
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

fn render_budget_manifest(
    config: &ProjectConfigSchema,
    portable: &str,
    codex: &str,
    claude: &str,
) -> Result<String, String> {
    let metric = |skill: &str,
                  token_budget: usize,
                  byte_budget: usize,
                  line_budget: usize|
     -> Result<Value, String> {
        let mut hasher = Sha256::new();
        hasher.update(skill.as_bytes());
        Ok(serde_json::json!({
            "tokens": count_skill_tokens(skill)?,
            "utf8_bytes": skill.len(),
            "lines": if skill.is_empty() { 0 } else { skill.lines().count() },
            "token_budget": token_budget,
            "utf8_byte_budget": byte_budget,
            "line_budget": line_budget,
            "sha256": format!("{:x}", hasher.finalize())
        }))
    };

    serde_json::to_string_pretty(&serde_json::json!({
        "schema_version": 1,
        "version": VERSION,
        "tokenizer": config.tokenizer.id,
        "runtime_core": {
            "ammodino": config.runtime_core.ammodino,
            "arranda": config.runtime_core.arranda
        },
        "skills": {
            "portable": metric(
                portable,
                config.budgets.portable.tokens,
                config.budgets.portable.utf8_bytes,
                config.budgets.portable.lines
            )?,
            "codex": metric(
                codex,
                config.budgets.codex.tokens,
                config.budgets.codex.utf8_bytes,
                config.budgets.codex.lines
            )?,
            "claude-code": metric(
                claude,
                config.budgets.claude_code.tokens,
                config.budgets.claude_code.utf8_bytes,
                config.budgets.claude_code.lines
            )?
        }
    }))
    .map(|json| format!("{json}\n"))
    .map_err(|error| format!("serialize token budget: {error}"))
}

fn render_schema_record() -> Result<String, String> {
    serialize_schema(schemars::schema_for!(Record))
}

fn render_schema_sources() -> Result<String, String> {
    serialize_schema(schemars::schema_for!(SourcesFile))
}

fn render_schema_grammar() -> Result<String, String> {
    serialize_schema(schemars::schema_for!(GrammarFile))
}

fn render_schema_config() -> Result<String, String> {
    serialize_schema(schemars::schema_for!(ProjectConfigSchema))
}

fn render_schema_package_manifest() -> Result<String, String> {
    serialize_schema(schemars::schema_for!(PackageManifestSchema))
}

fn serialize_schema(schema: schemars::schema::RootSchema) -> Result<String, String> {
    serde_json::to_string_pretty(&schema)
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("serialize JSON schema: {error}"))
}

pub(crate) fn package(root: &Path, args: &[String]) -> Result<(), String> {
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

    project_config_check(root)?;
    corpus_check(root)?;
    manifests::check(root)?;
    generate(root, true)?;
    bench::release_gates_pass(root, version)?;

    let dist = root.join("dist");
    fs::create_dir_all(&dist).map_err(|error| format!("create dist: {error}"))?;

    let staging = dist.join(format!(".toen-staging-{}", std::process::id()));
    if staging.exists() {
        return Err(format!(
            "package staging path already exists: {}",
            staging.display()
        ));
    }
    fs::create_dir(&staging)
        .map_err(|error| format!("create package staging directory: {error}"))?;

    let result = (|| {
        let skill_archive = staging.join(format!("toen-skill-v{version}.zip"));
        let codex_archive = staging.join(format!("toen-codex-plugin-v{version}.zip"));
        let claude_archive = staging.join(format!("toen-claude-code-plugin-v{version}.zip"));
        let evidence_archive = staging.join(format!("toen-benchmark-evidence-v{version}.zip"));
        let benchmark_report = staging.join(format!("toen-benchmark-report-v{version}.md"));

        packaging::write_zip_with_prefix(&skill_archive, root, "skill/toen", "toen")?;
        packaging::write_zip(&codex_archive, root, "plugins/codex/toen")?;
        packaging::write_zip(&claude_archive, root, "plugins/claude-code/toen")?;
        packaging::write_benchmark_zip(&evidence_archive, root, version)?;
        fs::copy(
            root.join("benchmarks/releases")
                .join(version)
                .join("report.md"),
            &benchmark_report,
        )
        .map_err(|error| format!("copy benchmark report: {error}"))?;

        let mut archives = [
            skill_archive,
            codex_archive,
            claude_archive,
            evidence_archive,
            benchmark_report,
        ];
        archives.sort_by(|left, right| left.file_name().cmp(&right.file_name()));
        let checksums = archives
            .iter()
            .map(|path| packaging::sha256_line(path))
            .collect::<Result<Vec<_>, _>>()?
            .join("");
        atomic_write(
            &staging.join(format!("toen-v{version}-checksums.txt")),
            checksums.as_bytes(),
        )?;

        let outputs = [
            format!("toen-skill-v{version}.zip"),
            format!("toen-codex-plugin-v{version}.zip"),
            format!("toen-claude-code-plugin-v{version}.zip"),
            format!("toen-benchmark-evidence-v{version}.zip"),
            format!("toen-benchmark-report-v{version}.md"),
            format!("toen-v{version}-checksums.txt"),
        ];
        packaging::replace_owned_outputs(&staging, &dist, &outputs)
    })();

    let cleanup = fs::remove_dir_all(&staging)
        .map_err(|error| format!("remove package staging directory: {error}"));
    match (result, cleanup) {
        (Err(error), _) => return Err(error),
        (Ok(()), Err(error)) => return Err(error),
        (Ok(()), Ok(())) => {}
    }

    println!(
        "package: wrote reproducible distributions, benchmark evidence, and checksums in dist/"
    );
    Ok(())
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

#[cfg(test)]
pub(crate) fn repo_root() -> Result<PathBuf, String> {
    workspace::Workspace::discover(None)
        .map(|workspace| workspace.root().to_path_buf())
        .map_err(|error| error.to_string())
}

pub(crate) fn atomic_write(path: &Path, contents: &[u8]) -> Result<(), String> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create output directory {}: {error}", parent.display()))?;
    }
    let temporary = path.with_file_name(format!(
        ".{}.tmp-{}",
        path.file_name()
            .and_then(|name| name.to_str())
            .unwrap_or("toen-output"),
        std::process::id()
    ));
    let result = (|| {
        let mut file = fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&temporary)
            .map_err(|error| format!("create {}: {error}", temporary.display()))?;
        std::io::Write::write_all(&mut file, contents)
            .map_err(|error| format!("write {}: {error}", temporary.display()))?;
        file.sync_all()
            .map_err(|error| format!("sync {}: {error}", temporary.display()))?;
        packaging::replace_file(&temporary, path)
    })();
    if result.is_err() {
        let _ = fs::remove_file(&temporary);
    }
    result
}

#[cfg(test)]
mod tests {
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
    fn configuration_and_workspace_validation_cover_native_discovery() {
        let root = repo_root().unwrap();
        project_config_check(&root).unwrap();
        assert_eq!(
            workspace::Workspace::discover(Some(&root)).unwrap().root(),
            root
        );
        assert!(
            workspace::Workspace::discover(Some(Path::new("/tmp/not-a-toen-workspace"))).is_err()
        );
        assert_eq!(error::message("typed failure"), "typed failure");
    }

    #[test]
    fn text_and_source_helpers_reject_invalid_values() {
        assert!(contains_wrong_de("dè").then_some(()).is_some());
        assert!(contains_wrong_de("de").then_some(()).is_none());
        assert!(source_url_matches(
            "https://example.test/",
            "https://example.test/page"
        ));
        assert!(!source_url_matches(
            "https://example.test",
            "https://other.test/page"
        ));
        assert!(validate_enum(Path::new("test"), "kind", "lexeme", &["lexeme"]).is_ok());
        assert!(validate_enum(Path::new("test"), "kind", "bad", &["lexeme"]).is_err());
        assert!(!valid_date("2026-02-30"));
    }

    #[test]
    fn atomic_writes_replace_existing_files() {
        let path = std::env::temp_dir().join(format!("toen-atomic-{}", std::process::id()));
        atomic_write(&path, b"first").unwrap();
        atomic_write(&path, b"second").unwrap();
        assert_eq!(fs::read_to_string(&path).unwrap(), "second");
        fs::remove_file(path).unwrap();
    }

    #[test]
    fn package_output_replacement_preserves_unrelated_dist_entries() {
        let root = std::env::temp_dir().join(format!("toen-package-output-{}", std::process::id()));
        let dist = root.join("dist");
        let staging = dist.join(".toen-staging-test");
        fs::create_dir_all(&staging).unwrap();
        fs::create_dir_all(dist.join("unrelated-directory")).unwrap();
        fs::write(dist.join("unrelated.txt"), "keep me").unwrap();
        fs::write(dist.join("owned.zip"), "old archive").unwrap();
        fs::write(staging.join("owned.zip"), "new archive").unwrap();

        packaging::replace_owned_outputs(&staging, &dist, &["owned.zip".to_owned()]).unwrap();

        assert_eq!(
            fs::read_to_string(dist.join("unrelated.txt")).unwrap(),
            "keep me"
        );
        assert!(dist.join("unrelated-directory").is_dir());
        assert_eq!(
            fs::read_to_string(dist.join("owned.zip")).unwrap(),
            "new archive"
        );
        assert!(
            !dist
                .join(format!(".toen-backup-{}", std::process::id()))
                .exists()
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_output_replacement_rolls_back_on_late_failure() {
        let root =
            std::env::temp_dir().join(format!("toen-package-rollback-{}", std::process::id()));
        let dist = root.join("dist");
        let staging = dist.join(".toen-staging-test");
        fs::create_dir_all(&staging).unwrap();
        fs::write(dist.join("first.zip"), "old first").unwrap();
        fs::create_dir(dist.join("second.zip")).unwrap();
        fs::write(staging.join("first.zip"), "new first").unwrap();
        fs::write(staging.join("second.zip"), "new second").unwrap();

        let error = packaging::replace_owned_outputs(
            &staging,
            &dist,
            &["first.zip".to_owned(), "second.zip".to_owned()],
        )
        .unwrap_err();

        assert!(error.contains("directory"));
        assert_eq!(
            fs::read_to_string(dist.join("first.zip")).unwrap(),
            "old first"
        );
        assert!(dist.join("second.zip").is_dir());
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn package_output_replacement_restores_backups_after_install_failure() {
        let root = std::env::temp_dir().join(format!(
            "toen-package-install-rollback-{}",
            std::process::id()
        ));
        let dist = root.join("dist");
        let staging = dist.join(".toen-staging-test");
        fs::create_dir_all(staging.join("nested")).unwrap();
        fs::write(dist.join("first.zip"), "old first").unwrap();
        fs::write(staging.join("first.zip"), "new first").unwrap();
        fs::write(staging.join("nested/second.zip"), "new second").unwrap();

        let error = packaging::replace_owned_outputs(
            &staging,
            &dist,
            &["first.zip".to_owned(), "nested/second.zip".to_owned()],
        )
        .unwrap_err();

        assert!(error.contains("install package output"));
        assert_eq!(
            fs::read_to_string(dist.join("first.zip")).unwrap(),
            "old first"
        );
        fs::remove_dir_all(root).unwrap();
    }

    #[test]
    fn command_and_process_boundaries_report_expected_results() {
        assert!(run(vec!["help".to_owned()]).is_ok());
        assert!(run(vec!["unknown".to_owned()]).is_err());
        assert!(run(vec!["--workspace".to_owned()]).is_err());
        assert!(
            run(vec![
                "sources".to_owned(),
                "verify".to_owned(),
                "--bad".to_owned()
            ])
            .is_err()
        );

        let root = repo_root().unwrap();
        assert!(run_command(&root, "rustc", &["--version"]).is_ok());
        assert!(run_command(&root, "__toen_command_does_not_exist", &[]).is_err());
        assert!(doctor(&root).is_ok());
        cli::usage(VERSION);
        assert!(command_available("rustc"));
        assert_eq!(
            optional_command_available("__toen_missing"),
            "not found (optional)"
        );
    }

    #[test]
    fn programmatic_workspace_argument_is_authoritative() {
        let result = run(vec![
            "--workspace".to_owned(),
            format!(
                "/tmp/toen-programmatic-not-a-workspace-{}",
                std::process::id()
            ),
            "doctor".to_owned(),
        ]);

        match result {
            Err(ToenError::Workspace(message)) => {
                assert!(message.contains("workspace"));
            }
            other => panic!("expected typed workspace error, got {other:?}"),
        }
    }

    #[test]
    fn malformed_source_and_priority_inputs_are_rejected() {
        let invalid = SourcesFile {
            source: vec![Source {
                id: String::new(),
                name: String::new(),
                url: HttpsUrl("http://invalid".to_owned()),
                archive_url: None,
                local_attestation: false,
            }],
        };
        assert!(sources::validate_catalog(&invalid).is_err());
        assert!(validate_runtime_priorities(&[], RuntimeMode::Ammodino, 1).is_err());
        assert!(validate_record_relationships(&[]).is_err());
        assert!(!manifests::has_explicit_invocation_policy(
            "policy:\n  allow_implicit_invocation: true\n"
        ));
        assert!(manifests::has_explicit_invocation_policy(
            "policy:\n  allow_implicit_invocation: false\n"
        ));
        assert!(valid_date("2026-04-30"));
        assert!(valid_date("2026-06-30"));
        assert!(valid_date("2026-09-30"));
        assert!(valid_date("2026-11-30"));
    }

    #[test]
    fn https_schema_matches_runtime_validation() {
        let schema = serde_json::to_value(schemars::schema_for!(Record)).unwrap();
        assert_eq!(schema["definitions"]["HttpsUrl"]["pattern"], "^https://");
        assert_eq!(schema["definitions"]["HttpsUrl"]["format"], "uri");
    }

    #[test]
    fn generated_text_and_source_notice_have_expected_content() {
        let root = repo_root().unwrap();
        let config = config::load(&root).unwrap();
        let records = load_records(&root).unwrap();
        let grammar = load_grammar(&root).unwrap();
        let skill = render_skill(
            &records,
            &grammar,
            config.runtime_core.ammodino,
            config.runtime_core.arranda,
        )
        .unwrap();
        let claude = render_claude_skill(
            &records,
            &grammar,
            config.runtime_core.ammodino,
            config.runtime_core.arranda,
        )
        .unwrap();
        assert!(skill.starts_with("---\nname: toen"));
        assert!(claude.contains("disable-model-invocation: true"));
        assert!(render_dictionary(&records).contains("Generated Dictionary"));
        assert!(
            render_source_notice(&root)
                .unwrap()
                .contains("Source Notice")
        );
        assert!(count_skill_tokens(&skill).unwrap() <= config.budgets.portable.tokens);
    }

    #[test]
    fn skill_budget_boundaries_are_enforced() {
        assert!(validate_skill_budget("test", "x", 1, 1, 1, 1).is_ok());
        assert!(validate_skill_budget("test", "xx", 1, 1, 1, 1).is_err());
        assert!(validate_skill_budget("test", "x", 2, 1, 1, 1).is_err());
        assert!(validate_skill_budget("test", "x\ny", 1, 2, 3, 1).is_err());
    }

    #[test]
    fn budget_manifest_is_deterministic() {
        let root = repo_root().unwrap();
        let config = config::load(&root).unwrap();
        let manifest = render_budget_manifest(&config, "portable", "codex", "claude").unwrap();

        assert!(manifest.contains("750"));
        assert!(manifest.contains("ammodino"));
        assert!(manifest.contains("o200k-base"));
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
    fn portable_skill_archives_are_reproducible_and_self_contained() {
        let root = repo_root().unwrap();
        let first = std::env::temp_dir().join(format!(
            "toen-skill-policy-first-{}.zip",
            std::process::id()
        ));
        let second = std::env::temp_dir().join(format!(
            "toen-skill-policy-second-{}.zip",
            std::process::id()
        ));

        packaging::write_zip_with_prefix(&first, &root, "skill/toen", "toen").unwrap();
        packaging::write_zip_with_prefix(&second, &root, "skill/toen", "toen").unwrap();

        assert_eq!(fs::read(&first).unwrap(), fs::read(&second).unwrap());

        let archive = fs::File::open(&first).unwrap();
        let mut archive = ZipArchive::new(archive).unwrap();
        assert!(archive.by_name("toen/SKILL.md").is_ok());
        assert!(archive.by_name("toen/README.md").is_ok());
        assert!(archive.by_name("toen/LICENSE").is_ok());
        assert!(archive.by_name("toen/CORPUS-LICENSE.md").is_ok());
        assert!(archive.by_name("toen/SOURCE-NOTICE.md").is_ok());

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

        packaging::write_zip(&first, &root, "plugins/codex/toen").unwrap();
        packaging::write_zip(&second, &root, "plugins/codex/toen").unwrap();

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

    #[test]
    fn claude_skill_is_explicit_only_and_has_a_host_argument_contract() {
        let skill = fs::read_to_string(
            repo_root()
                .unwrap()
                .join("plugins/claude-code/toen/skills/toen/SKILL.md"),
        )
        .unwrap();
        assert!(skill.contains("disable-model-invocation: true"));
        assert!(skill.contains("argument-hint:"));
        assert!(skill.contains("/toen:toen"));
    }
}
