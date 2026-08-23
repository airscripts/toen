use std::fs;
use std::io::{self, Read};
use std::path::{Path, PathBuf};

use clap::{Parser, Subcommand, ValueEnum, error::ErrorKind};
use schemars::JsonSchema;
use serde::Serialize;
use sha2::{Digest, Sha256};
use tiktoken_rs::{CoreBPE, o200k_base};

pub(crate) const TOKENIZER_ID: &str = "o200k-base";
const SCHEMA_VERSION: u32 = 1;

#[derive(Debug, Parser)]
#[command(
    name = "toenctl toenizer",
    about = "Deterministic local token estimation"
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Count {
        #[arg(long, conflicts_with = "file")]
        text: Option<String>,
        #[arg(long, conflicts_with = "text")]
        file: Option<PathBuf>,
        #[arg(long, default_value = TOKENIZER_ID)]
        tokenizer: String,
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },
    Compare {
        #[arg(long)]
        baseline: String,
        #[arg(long)]
        candidate: String,
        #[arg(long, default_value = TOKENIZER_ID)]
        tokenizer: String,
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },
    Report {
        #[arg(long)]
        check: bool,
        #[arg(long, default_value = TOKENIZER_ID)]
        tokenizer: String,
        #[arg(long, value_enum, default_value_t = Format::Human)]
        format: Format,
    },
}

#[derive(Clone, Copy, Debug, ValueEnum)]
enum Format {
    Human,
    Json,
}

#[derive(Debug, JsonSchema, Serialize)]
struct Metrics {
    schema_version: u32,
    tokenizer: &'static str,
    token_estimate: usize,
    utf8_bytes: usize,
    lines: usize,
}

#[derive(Debug, JsonSchema, Serialize)]
struct Comparison {
    schema_version: u32,
    tokenizer: &'static str,
    baseline: Metrics,
    candidate: Metrics,
    signed_token_difference: i64,
    estimated_saving_percent: Option<f64>,
}

#[derive(Debug, JsonSchema, Serialize)]
struct SkillMetrics {
    path: String,
    sha256: String,
    tokenizer: &'static str,
    token_estimate: usize,
    utf8_bytes: usize,
    lines: usize,
    token_budget: usize,
    utf8_byte_budget: usize,
    line_budget: usize,
}

#[derive(Debug, JsonSchema, Serialize)]
struct ExampleMetrics {
    record_id: String,
    example_index: usize,
    italian: Metrics,
    livornese: Metrics,
    signed_token_difference: i64,
    estimated_saving_percent: Option<f64>,
}

#[derive(Debug, JsonSchema, Serialize)]
struct ReportTotals {
    example_count: usize,
    italian_tokens: usize,
    livornese_tokens: usize,
    signed_token_difference: i64,
    estimated_saving_percent: Option<f64>,
    paired_median_saving_percent: Option<f64>,
    shorter_count: usize,
    equal_count: usize,
    longer_count: usize,
}

#[derive(Debug, JsonSchema, Serialize)]
pub(crate) struct Report {
    schema_version: u32,
    tokenizer: &'static str,
    skills: Vec<SkillMetrics>,
    examples: Vec<ExampleMetrics>,
    totals: ReportTotals,
}

#[derive(Debug, JsonSchema)]
#[schemars(untagged)]
#[allow(dead_code)]
enum ToenizerOutput {
    Metrics(Metrics),
    Comparison(Comparison),
    Report(Report),
}

pub(crate) fn run(args: &[String], root: Option<&Path>) -> Result<(), String> {
    let cli = match Cli::try_parse_from(
        std::iter::once("toenizer".to_owned()).chain(args.iter().cloned()),
    ) {
        Ok(cli) => cli,
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            print!("{error}");
            return Ok(());
        }
        Err(error) => return Err(error.to_string()),
    };

    match cli.command {
        Command::Count {
            text,
            file,
            tokenizer,
            format,
        } => {
            ensure_tokenizer(&tokenizer)?;
            let input = match (text, file) {
                (Some(text), None) => text,
                (None, Some(path)) => read_input(&path)?,
                (None, None) => return Err("count requires --text or --file".to_owned()),
                (Some(_), Some(_)) => unreachable!("clap enforces mutually exclusive inputs"),
            };
            print_metrics(&metrics(&input)?, format)
        }
        Command::Compare {
            baseline,
            candidate,
            tokenizer,
            format,
        } => {
            ensure_tokenizer(&tokenizer)?;
            print_comparison(&comparison(&baseline, &candidate)?, format)
        }
        Command::Report {
            check,
            tokenizer,
            format,
        } => {
            ensure_tokenizer(&tokenizer)?;
            let root = root.ok_or_else(|| "toenizer report requires a workspace".to_owned())?;
            let report = build_report(root)?;
            if check {
                check_report(root, &report)?;
            } else if matches!(format, Format::Json) {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&report)
                        .map_err(|error| format!("serialize report: {error}"))?
                );
            } else {
                write_report(root, &report)?;
                println!("toenizer: wrote docs/toenizer-report.md and docs/toenizer-report.json");
            }
            Ok(())
        }
    }
}

pub(crate) fn is_display_request(args: &[String]) -> bool {
    matches!(
        Cli::try_parse_from(std::iter::once("toenizer".to_owned()).chain(args.iter().cloned())),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            )
    )
}

pub(crate) fn validate_args(args: &[String]) -> Result<(), String> {
    match Cli::try_parse_from(std::iter::once("toenizer".to_owned()).chain(args.iter().cloned())) {
        Ok(_) => Ok(()),
        Err(error)
            if matches!(
                error.kind(),
                ErrorKind::DisplayHelp | ErrorKind::DisplayVersion
            ) =>
        {
            Ok(())
        }
        Err(error) => Err(error.to_string()),
    }
}

pub(crate) fn render_schema() -> Result<String, String> {
    let mut schema = serde_json::to_value(schemars::schema_for!(ToenizerOutput))
        .map_err(|error| format!("serialize Toenizer schema: {error}"))?;
    if let Some(object) = schema.as_object_mut() {
        if let Some(branches) = object.remove("anyOf") {
            object.insert("oneOf".to_owned(), branches);
        }
        object.insert(
            "$schema".to_owned(),
            serde_json::Value::String("http://json-schema.org/draft-07/schema#".to_owned()),
        );
        object.insert(
            "title".to_owned(),
            serde_json::Value::String("ToenizerOutput".to_owned()),
        );
    }
    serde_json::to_string_pretty(&schema)
        .map(|json| format!("{json}\n"))
        .map_err(|error| format!("serialize Toenizer schema: {error}"))
}

fn ensure_tokenizer(tokenizer: &str) -> Result<(), String> {
    if tokenizer == TOKENIZER_ID {
        Ok(())
    } else {
        Err(format!(
            "unsupported tokenizer {tokenizer}; only {TOKENIZER_ID} is available"
        ))
    }
}

fn read_input(path: &Path) -> Result<String, String> {
    if path == Path::new("-") {
        let mut input = String::new();
        io::stdin()
            .read_to_string(&mut input)
            .map_err(|error| format!("read stdin: {error}"))?;
        Ok(input)
    } else {
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))
    }
}

fn metrics(text: &str) -> Result<Metrics, String> {
    let tokenizer = o200k_base().map_err(|error| format!("load o200k_base tokenizer: {error}"))?;
    Ok(metrics_with_tokenizer(&tokenizer, text))
}

fn metrics_with_tokenizer(tokenizer: &CoreBPE, text: &str) -> Metrics {
    Metrics {
        schema_version: SCHEMA_VERSION,
        tokenizer: TOKENIZER_ID,
        token_estimate: tokenizer.encode_with_special_tokens(text).len(),
        utf8_bytes: text.len(),
        lines: if text.is_empty() {
            0
        } else {
            text.lines().count()
        },
    }
}

fn comparison(baseline: &str, candidate: &str) -> Result<Comparison, String> {
    let tokenizer = o200k_base().map_err(|error| format!("load o200k_base tokenizer: {error}"))?;
    Ok(comparison_with_tokenizer(&tokenizer, baseline, candidate))
}

fn comparison_with_tokenizer(tokenizer: &CoreBPE, baseline: &str, candidate: &str) -> Comparison {
    let baseline = metrics_with_tokenizer(tokenizer, baseline);
    let candidate = metrics_with_tokenizer(tokenizer, candidate);
    let difference = baseline.token_estimate as i64 - candidate.token_estimate as i64;
    let estimated_saving_percent = if baseline.token_estimate == 0 {
        None
    } else {
        Some(round_percent(
            difference as f64 * 100.0 / baseline.token_estimate as f64,
        ))
    };

    Comparison {
        schema_version: SCHEMA_VERSION,
        tokenizer: TOKENIZER_ID,
        baseline,
        candidate,
        signed_token_difference: difference,
        estimated_saving_percent,
    }
}

fn print_metrics(value: &Metrics, format: Format) -> Result<(), String> {
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(value)
                .map_err(|error| format!("serialize metrics: {error}"))?
        ),
        Format::Human => {
            println!("Tokenizer: {}", value.tokenizer);
            println!("Token Estimate: {}", value.token_estimate);
            println!("UTF-8 Bytes: {}", value.utf8_bytes);
            println!("Lines: {}", value.lines);
        }
    }
    Ok(())
}

fn print_comparison(value: &Comparison, format: Format) -> Result<(), String> {
    match format {
        Format::Json => println!(
            "{}",
            serde_json::to_string_pretty(value)
                .map_err(|error| format!("serialize comparison: {error}"))?
        ),
        Format::Human => {
            println!("Tokenizer: {}", value.tokenizer);
            println!("Baseline Tokens: {}", value.baseline.token_estimate);
            println!("Candidate Tokens: {}", value.candidate.token_estimate);
            println!("Signed Token Difference: {}", value.signed_token_difference);
            match value.estimated_saving_percent {
                Some(value) if value < 0.0 => println!("Estimated Saving: {value:.2}% (increase)"),
                Some(value) => println!("Estimated Saving: {value:.2}%"),
                None => println!("Estimated Saving: n/a"),
            }
            println!("Baseline UTF-8 Bytes: {}", value.baseline.utf8_bytes);
            println!("Candidate UTF-8 Bytes: {}", value.candidate.utf8_bytes);
            println!("Baseline Lines: {}", value.baseline.lines);
            println!("Candidate Lines: {}", value.candidate.lines);
        }
    }
    Ok(())
}

pub(crate) fn build_report(root: &Path) -> Result<Report, String> {
    let tokenizer = o200k_base().map_err(|error| format!("load o200k_base tokenizer: {error}"))?;
    let config = crate::config::load(root)?;
    let variants = [
        (
            "portable",
            "skill/toen/SKILL.md",
            config.budgets.portable.tokens,
            config.budgets.portable.utf8_bytes,
            config.budgets.portable.lines,
        ),
        (
            "codex",
            "plugins/codex/toen/skills/toen/SKILL.md",
            config.budgets.codex.tokens,
            config.budgets.codex.utf8_bytes,
            config.budgets.codex.lines,
        ),
        (
            "claude-code",
            "plugins/claude-code/toen/skills/toen/SKILL.md",
            config.budgets.claude_code.tokens,
            config.budgets.claude_code.utf8_bytes,
            config.budgets.claude_code.lines,
        ),
    ];
    let skills = variants
        .into_iter()
        .map(|(_, path, token_budget, byte_budget, line_budget)| {
            let absolute = root.join(path);
            let text = fs::read_to_string(&absolute)
                .map_err(|error| format!("read {}: {error}", absolute.display()))?;
            let metric = metrics_with_tokenizer(&tokenizer, &text);
            let mut hasher = Sha256::new();
            hasher.update(text.as_bytes());
            Ok(SkillMetrics {
                path: path.to_owned(),
                sha256: format!("{:x}", hasher.finalize()),
                tokenizer: TOKENIZER_ID,
                token_estimate: metric.token_estimate,
                utf8_bytes: metric.utf8_bytes,
                lines: metric.lines,
                token_budget,
                utf8_byte_budget: byte_budget,
                line_budget,
            })
        })
        .collect::<Result<Vec<_>, String>>()?;

    let records = crate::load_records(root)?;
    let mut examples = Vec::new();
    for record in records {
        for (example_index, example) in record.examples.iter().enumerate() {
            let comparison =
                comparison_with_tokenizer(&tokenizer, &example.italian, &example.livornese);
            examples.push(ExampleMetrics {
                record_id: record.id.clone(),
                example_index,
                italian: comparison.baseline,
                livornese: comparison.candidate,
                signed_token_difference: comparison.signed_token_difference,
                estimated_saving_percent: comparison.estimated_saving_percent,
            });
        }
    }

    let italian_tokens = examples
        .iter()
        .map(|item| item.italian.token_estimate)
        .sum();
    let livornese_tokens = examples
        .iter()
        .map(|item| item.livornese.token_estimate)
        .sum();
    let signed_difference = italian_tokens as i64 - livornese_tokens as i64;
    let percentages = examples
        .iter()
        .filter_map(|item| item.estimated_saving_percent)
        .collect::<Vec<_>>();
    let mut sorted_percentages = percentages.clone();
    sorted_percentages.sort_by(f64::total_cmp);
    let median = if sorted_percentages.is_empty() {
        None
    } else {
        let middle = sorted_percentages.len() / 2;
        Some(if sorted_percentages.len().is_multiple_of(2) {
            round_percent((sorted_percentages[middle - 1] + sorted_percentages[middle]) / 2.0)
        } else {
            sorted_percentages[middle]
        })
    };
    let shorter_count = examples
        .iter()
        .filter(|item| item.signed_token_difference > 0)
        .count();
    let equal_count = examples
        .iter()
        .filter(|item| item.signed_token_difference == 0)
        .count();
    let longer_count = examples
        .iter()
        .filter(|item| item.signed_token_difference < 0)
        .count();
    let example_count = examples.len();

    Ok(Report {
        schema_version: SCHEMA_VERSION,
        tokenizer: TOKENIZER_ID,
        skills,
        examples,
        totals: ReportTotals {
            example_count,
            italian_tokens,
            livornese_tokens,
            signed_token_difference: signed_difference,
            estimated_saving_percent: (italian_tokens != 0)
                .then(|| round_percent(signed_difference as f64 * 100.0 / italian_tokens as f64)),
            paired_median_saving_percent: median,
            shorter_count,
            equal_count,
            longer_count,
        },
    })
}

pub(crate) fn write_report(root: &Path, report: &Report) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| format!("serialize toenizer report: {error}"))?;
    crate::atomic_write(
        &root.join("docs/toenizer-report.json"),
        format!("{json}\n").as_bytes(),
    )?;
    crate::atomic_write(
        &root.join("docs/toenizer-report.md"),
        render_markdown(report).as_bytes(),
    )?;
    Ok(())
}

pub(crate) fn check_report(root: &Path, report: &Report) -> Result<(), String> {
    let json = serde_json::to_string_pretty(report)
        .map_err(|error| format!("serialize toenizer report: {error}"))?;
    let expected_json = format!("{json}\n");
    let expected_markdown = render_markdown(report);
    for (path, expected) in [
        (root.join("docs/toenizer-report.json"), expected_json),
        (root.join("docs/toenizer-report.md"), expected_markdown),
    ] {
        let current = fs::read_to_string(&path)
            .map_err(|error| format!("read {}: {error}", path.display()))?;
        if current != expected {
            return Err(format!(
                "{} is out of date; run `toenctl toenizer report`",
                path.display()
            ));
        }
    }
    println!("toenizer: reports are up to date");
    Ok(())
}

fn render_markdown(report: &Report) -> String {
    let mut output = String::from(
        "<!-- Generated File—Do Not Edit. -->\n# Toenizer Report\n\nToenizer provides deterministic local token estimates using `o200k-base`; it does not report provider usage or billing.\n\n## Skill Variants\n\n| Variant | Tokens | Budget | UTF-8 Bytes | Budget | Lines | Budget | SHA-256 |\n| --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n",
    );
    for skill in &report.skills {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} | {} | `{}` |\n",
            skill.path,
            skill.token_estimate,
            skill.token_budget,
            skill.utf8_bytes,
            skill.utf8_byte_budget,
            skill.lines,
            skill.line_budget,
            skill.sha256
        ));
    }
    output.push_str(&format!(
        "\n## Corpus Examples\n\n{} paired Italian-versus-Livornese examples were measured. Aggregate Italian tokens: **{}**. Livornese tokens: **{}**. Signed difference: **{}**. Estimated Saving: **{}**. Paired median saving: **{}**. Shorter/equal/longer: **{}/{}/{}**.\n\n| Record | Example | Italian Tokens | Livornese Tokens | Signed Difference | Estimated Saving |\n| --- | ---: | ---: | ---: | ---: | ---: |\n",
        report.totals.example_count,
        report.totals.italian_tokens,
        report.totals.livornese_tokens,
        report.totals.signed_token_difference,
        format_percent(report.totals.estimated_saving_percent),
        format_percent(report.totals.paired_median_saving_percent),
        report.totals.shorter_count,
        report.totals.equal_count,
        report.totals.longer_count
    ));
    for example in &report.examples {
        output.push_str(&format!(
            "| {} | {} | {} | {} | {} | {} |\n",
            example.record_id,
            example.example_index + 1,
            example.italian.token_estimate,
            example.livornese.token_estimate,
            example.signed_token_difference,
            format_percent(example.estimated_saving_percent)
        ));
    }
    output.push_str(
        "\n## Methodology And Limitations\n\nToenizer counts the exact UTF-8 input supplied by the caller. It performs no Unicode normalization, rewriting, or provider request. `o200k-base` is the disclosed estimation engine, not a universal standard, and its count is not a Claude or other provider tokenizer claim. Corpus savings are informational; releases gate determinism and size budgets, never a minimum saving percentage.\n",
    );
    output
}

fn format_percent(value: Option<f64>) -> String {
    value.map_or_else(|| "n/a".to_owned(), |value| format!("{value:.2}%"))
}

fn round_percent(value: f64) -> f64 {
    (value * 100.0).round() / 100.0
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    #[test]
    fn metrics_keep_exact_unicode_and_empty_input() {
        let accented = metrics("città").unwrap();
        assert_eq!(accented.utf8_bytes, 6);
        assert_eq!(accented.lines, 1);

        let empty = metrics("").unwrap();
        assert_eq!(empty.token_estimate, 0);
        assert_eq!(empty.utf8_bytes, 0);
        assert_eq!(empty.lines, 0);

        let multiline = metrics("a\nb\n").unwrap();
        assert_eq!(multiline.lines, 2);
    }

    #[test]
    fn comparison_preserves_negative_and_zero_baselines() {
        let longer = comparison("short", "this is much longer").unwrap();
        assert!(longer.signed_token_difference < 0);
        assert!(longer.estimated_saving_percent.unwrap() < 0.0);

        let zero = comparison("", "candidate").unwrap();
        assert_eq!(zero.estimated_saving_percent, None);
    }

    #[test]
    fn human_and_json_renderers_cover_all_output_branches() {
        let metric = metrics("test").unwrap();
        print_metrics(&metric, Format::Human).unwrap();
        print_metrics(&metric, Format::Json).unwrap();
        let value = comparison("short", "longer candidate").unwrap();
        print_comparison(&value, Format::Human).unwrap();
        print_comparison(&value, Format::Json).unwrap();
        let zero = comparison("", "candidate").unwrap();
        print_comparison(&zero, Format::Human).unwrap();
    }

    #[test]
    fn report_and_schema_are_deterministic() {
        let root = crate::repo_root().unwrap();
        let report = build_report(&root).unwrap();
        assert_eq!(report.schema_version, 1);
        assert!(report.totals.example_count > 0);
        assert!(render_markdown(&report).contains("Methodology And Limitations"));
        let schema: serde_json::Value = serde_json::from_str(&render_schema().unwrap()).unwrap();
        let definitions = schema["definitions"].as_object().unwrap();
        let branches = schema["oneOf"].as_array().unwrap();
        assert_eq!(branches.len(), 3);
        for branch in branches {
            let reference = branch["$ref"].as_str().unwrap();
            assert!(definitions.contains_key(reference.trim_start_matches("#/definitions/")));
        }
        write_report(&root, &report).unwrap();
        check_report(&root, &report).unwrap();
    }

    #[test]
    fn file_input_and_validation_errors_are_reported() {
        let path = std::env::temp_dir().join(format!("toenizer-input-{}", std::process::id()));
        fs::write(&path, "file input").unwrap();
        assert_eq!(read_input(&path).unwrap(), "file input");
        fs::remove_file(&path).unwrap();

        assert!(ensure_tokenizer("other").is_err());
        assert_eq!(format_percent(None), "n/a");
        assert_eq!(format_percent(Some(-2.5)), "-2.50%");
        assert_eq!(round_percent(12.345), 12.35);
    }
}
