use std::collections::HashMap;
use std::fs;
use std::io::{BufRead, BufReader};
use std::path::{Path, PathBuf};
use std::process::{Command, Output, Stdio};
use std::sync::LazyLock;
use std::thread;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};
use serde_json::Value;
use sha2::{Digest, Sha256};
use tiktoken_rs::{CoreBPE, o200k_base};

const RELEASE_SCENARIO_COUNT: usize = 54;
const RELEASE_SESSION_COUNT: usize = 6;
const RELEASE_REPETITIONS: usize = 3;
const RELEASE_JUDGED_REPLIES: usize = RELEASE_SCENARIO_COUNT + RELEASE_SESSION_COUNT * 10;
const CONDITIONS: [&str; 4] = ["italian", "terse_italian", "ammodino", "arranda"];
const MODELS: [&str; 2] = ["gpt-5.6-sol", "gpt-5.6-luna"];
static VISIBLE_TOKENIZER: LazyLock<Result<CoreBPE, String>> =
    LazyLock::new(|| o200k_base().map_err(|error| format!("load o200k_base tokenizer: {error}")));

#[derive(Debug, Clone, Deserialize, Serialize)]
struct Scenario {
    id: String,
    language: String,
    kind: String,
    prompt: String,
    #[serde(default)]
    protected: Vec<String>,
    fixture: Option<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SessionScenario {
    id: String,
    language: String,
    kind: String,
    turns: Vec<SessionTurn>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
struct SessionTurn {
    prompt: String,
    #[serde(default)]
    protected: Vec<String>,
}

#[derive(Debug, Deserialize)]
struct FixtureMetadata {
    test_command: Vec<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct TurnResult {
    turn: usize,
    prompt: String,
    visible_output: String,
    visible_output_tokens: u64,
    input_tokens: u64,
    output_tokens: u64,
    session_id: Option<String>,
    compacted: bool,
    protected: Vec<String>,
    protected_preserved: bool,
    fixture_test_passed: Option<bool>,
    fixture_test_output: Option<String>,
    stderr: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct CampaignRun {
    schema_version: u32,
    release: String,
    model: String,
    condition: String,
    scenario_id: String,
    language: String,
    kind: String,
    repetition: usize,
    session: bool,
    completed: bool,
    codex_version: String,
    reasoning: String,
    turns: Vec<TurnResult>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct BlindJudgeInput {
    id: String,
    benchmark_model: String,
    target_mode: String,
    scenario_id: String,
    repetition: usize,
    session: bool,
    turn: usize,
    task: String,
    protected: Vec<String>,
    output_a: String,
    output_b: String,
}

#[derive(Debug, Deserialize)]
struct JudgeScores {
    correctness_a: u8,
    correctness_b: u8,
    style_a: u8,
    style_b: u8,
    safety_violation_a: bool,
    safety_violation_b: bool,
    notes: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
struct JudgedPair {
    id: String,
    benchmark_model: String,
    judge_model: String,
    judge_codex_version: String,
    target_mode: String,
    scenario_id: String,
    repetition: usize,
    session: bool,
    turn: usize,
    target_side: String,
    correctness_target: f64,
    correctness_terse: f64,
    style_target: u8,
    style_terse: u8,
    safety_violation_target: bool,
    safety_violation_terse: bool,
    judge_input_tokens: u64,
    judge_output_tokens: u64,
    raw_judge_output: String,
    judge_stderr: String,
    notes: String,
}

#[derive(Debug, Deserialize)]
struct CompatibilityScores {
    chooser: bool,
    ammodino: bool,
    arranda: bool,
    status_de: bool,
    spengi: bool,
    inline_task: bool,
    switching: bool,
    new_session_reset: bool,
    resume: bool,
    compaction: bool,
    notes: String,
}

#[derive(Debug)]
struct ExecResult {
    visible_output: String,
    final_output: String,
    input_tokens: u64,
    output_tokens: u64,
    session_id: Option<String>,
    compacted: bool,
    stderr: String,
}

#[derive(Debug)]
struct Invocation<'a> {
    model: &'a str,
    prompt: &'a str,
    working_dir: &'a Path,
    writable: bool,
    persistent: bool,
    output_schema: Option<&'a Path>,
}

trait HarnessAdapter: Sync {
    fn name(&self) -> &'static str;

    fn version(&self) -> Result<String, String>;

    fn invoke(&self, invocation: &Invocation<'_>) -> Result<ExecResult, String>;

    fn resume(
        &self,
        model: &str,
        session_id: &str,
        prompt: &str,
        output_schema: Option<&Path>,
    ) -> Result<ExecResult, String>;

    fn invoke_configured(
        &self,
        invocation: &Invocation<'_>,
        extra_config: &[&str],
    ) -> Result<ExecResult, String>;

    fn resume_configured(
        &self,
        model: &str,
        session_id: &str,
        prompt: &str,
        output_schema: Option<&Path>,
        extra_config: &[&str],
    ) -> Result<ExecResult, String>;

    fn parse_events(&self, stdout: &[u8], stderr: &[u8]) -> Result<ExecResult, String>;
}

struct CodexAdapter {
    binary: String,
}

impl CodexAdapter {
    fn from_environment() -> Self {
        Self {
            binary: std::env::var("TOENCTL_CODEX_BIN").unwrap_or_else(|_| "codex".to_owned()),
        }
    }

    fn common_args(&self, model: &str) -> Vec<String> {
        vec![
            "--json".to_owned(),
            "--model".to_owned(),
            model.to_owned(),
            "--ignore-user-config".to_owned(),
            "--ignore-rules".to_owned(),
            "--skip-git-repo-check".to_owned(),
            "-c".to_owned(),
            "model_reasoning_effort=\"medium\"".to_owned(),
        ]
    }

    fn run_command(&self, args: &[String], stream_events: bool) -> Result<Output, String> {
        if !stream_events {
            return Command::new(&self.binary)
                .args(args)
                .stdin(Stdio::null())
                .output()
                .map_err(|error| format!("run {}: {error}", self.binary));
        }

        let mut child = Command::new(&self.binary)
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .spawn()
            .map_err(|error| format!("run {}: {error}", self.binary))?;
        let stdout = child
            .stdout
            .take()
            .ok_or_else(|| format!("capture {} stdout", self.binary))?;
        let stderr = child
            .stderr
            .take()
            .ok_or_else(|| format!("capture {} stderr", self.binary))?;
        let stderr_thread = thread::spawn(move || -> std::io::Result<Vec<u8>> {
            let mut reader = BufReader::new(stderr);
            let mut captured = Vec::new();
            let mut line = String::new();

            loop {
                line.clear();

                if reader.read_line(&mut line)? == 0 {
                    break;
                }

                eprint!("bench codex: {line}");
                captured.extend_from_slice(line.as_bytes());
            }

            Ok(captured)
        });
        let mut reader = BufReader::new(stdout);
        let mut captured_stdout = Vec::new();
        let mut line = String::new();

        loop {
            line.clear();

            if reader
                .read_line(&mut line)
                .map_err(|error| format!("read {} stdout: {error}", self.binary))?
                == 0
            {
                break;
            }

            log_codex_event(&line);
            captured_stdout.extend_from_slice(line.as_bytes());
        }

        let status = child
            .wait()
            .map_err(|error| format!("wait for {}: {error}", self.binary))?;
        let captured_stderr = stderr_thread
            .join()
            .map_err(|_| format!("{} stderr reader panicked", self.binary))?
            .map_err(|error| format!("read {} stderr: {error}", self.binary))?;

        Ok(Output {
            status,
            stdout: captured_stdout,
            stderr: captured_stderr,
        })
    }

    fn invoke_with_extra_config(
        &self,
        invocation: &Invocation<'_>,
        extra_config: &[&str],
    ) -> Result<ExecResult, String> {
        let mut args = vec!["exec".to_owned()];
        args.extend(self.common_args(invocation.model));

        for config in extra_config {
            args.extend(["-c".to_owned(), (*config).to_owned()]);
        }

        if invocation.writable {
            args.push("--approve-for-me".to_owned());
        } else {
            args.extend(["--sandbox".to_owned(), "read-only".to_owned()]);
        }

        args.extend([
            "--cd".to_owned(),
            invocation.working_dir.display().to_string(),
        ]);

        if !invocation.persistent {
            args.push("--ephemeral".to_owned());
        }

        if let Some(schema) = invocation.output_schema {
            args.extend(["--output-schema".to_owned(), schema.display().to_string()]);
        }

        args.push(invocation.prompt.to_owned());

        let output = self.run_command(&args, true)?;

        if !output.status.success() {
            return Err(format!(
                "codex exec failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        self.parse_events(&output.stdout, &output.stderr)
    }

    fn resume_with_extra_config(
        &self,
        model: &str,
        session_id: &str,
        prompt: &str,
        output_schema: Option<&Path>,
        extra_config: &[&str],
    ) -> Result<ExecResult, String> {
        let mut args = vec!["exec".to_owned(), "resume".to_owned()];
        args.extend(self.common_args(model));

        for config in extra_config {
            args.extend(["-c".to_owned(), (*config).to_owned()]);
        }

        if let Some(schema) = output_schema {
            args.extend(["--output-schema".to_owned(), schema.display().to_string()]);
        }

        args.extend([session_id.to_owned(), prompt.to_owned()]);

        let output = self.run_command(&args, true)?;

        if !output.status.success() {
            return Err(format!(
                "codex resume failed with {}: {}",
                output.status,
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        self.parse_events(&output.stdout, &output.stderr)
    }
}

impl HarnessAdapter for CodexAdapter {
    fn name(&self) -> &'static str {
        "codex"
    }

    fn version(&self) -> Result<String, String> {
        let output = self.run_command(&["--version".to_owned()], false)?;

        if !output.status.success() {
            return Err(format!(
                "codex --version failed: {}",
                String::from_utf8_lossy(&output.stderr).trim()
            ));
        }

        Ok(String::from_utf8_lossy(&output.stdout).trim().to_owned())
    }

    fn invoke(&self, invocation: &Invocation<'_>) -> Result<ExecResult, String> {
        self.invoke_with_extra_config(invocation, &[])
    }

    fn resume(
        &self,
        model: &str,
        session_id: &str,
        prompt: &str,
        output_schema: Option<&Path>,
    ) -> Result<ExecResult, String> {
        self.resume_with_extra_config(model, session_id, prompt, output_schema, &[])
    }

    fn invoke_configured(
        &self,
        invocation: &Invocation<'_>,
        extra_config: &[&str],
    ) -> Result<ExecResult, String> {
        self.invoke_with_extra_config(invocation, extra_config)
    }

    fn resume_configured(
        &self,
        model: &str,
        session_id: &str,
        prompt: &str,
        output_schema: Option<&Path>,
        extra_config: &[&str],
    ) -> Result<ExecResult, String> {
        self.resume_with_extra_config(model, session_id, prompt, output_schema, extra_config)
    }

    fn parse_events(&self, stdout: &[u8], stderr: &[u8]) -> Result<ExecResult, String> {
        let mut visible_messages = Vec::new();
        let mut session_id = None;
        let mut usage = None;
        let mut compacted = false;

        for (index, line) in String::from_utf8_lossy(stdout).lines().enumerate() {
            if line.trim().is_empty() {
                continue;
            }

            let event: Value = serde_json::from_str(line)
                .map_err(|error| format!("parse Codex JSONL line {}: {error}", index + 1))?;

            if event.get("type").and_then(Value::as_str) == Some("thread.started") {
                session_id = event
                    .get("thread_id")
                    .and_then(Value::as_str)
                    .map(str::to_owned);
            }

            if event.get("type").and_then(Value::as_str) == Some("item.completed")
                && event
                    .get("item")
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    == Some("agent_message")
                && let Some(message) = event
                    .get("item")
                    .and_then(|item| item.get("text"))
                    .and_then(Value::as_str)
            {
                visible_messages.push(message.to_owned());
            }

            if event.get("type").and_then(Value::as_str) == Some("context.compacted")
                || event.get("type").and_then(Value::as_str) == Some("context_compaction")
                || event
                    .get("item")
                    .and_then(|item| item.get("type"))
                    .and_then(Value::as_str)
                    == Some("context_compaction")
            {
                compacted = true;
            }

            if let Some(value) = event.get("usage") {
                let input = value.get("input_tokens").and_then(Value::as_u64);
                let output = value.get("output_tokens").and_then(Value::as_u64);

                if let (Some(input), Some(output)) = (input, output) {
                    usage = Some((input, output));
                }
            }
        }

        if visible_messages.is_empty() {
            return Err("Codex JSONL did not contain a completed agent message".to_owned());
        }

        let final_output = visible_messages.last().cloned().unwrap_or_default();
        let visible_output = visible_messages.join("\n\n");
        let (input_tokens, output_tokens) =
            usage.ok_or_else(|| "Codex JSONL did not contain provider usage".to_owned())?;

        Ok(ExecResult {
            visible_output,
            final_output,
            input_tokens,
            output_tokens,
            session_id,
            compacted,
            stderr: String::from_utf8_lossy(stderr).to_string(),
        })
    }
}

fn log_codex_event(line: &str) {
    let Ok(event) = serde_json::from_str::<Value>(line) else {
        println!("bench codex: received non-JSON output");
        return;
    };
    let event_type = event
        .get("type")
        .and_then(Value::as_str)
        .unwrap_or("unknown event");

    if event_type == "item.completed" {
        let item_type = event
            .get("item")
            .and_then(|item| item.get("type"))
            .and_then(Value::as_str)
            .unwrap_or("unknown item");
        println!("bench codex: {event_type} ({item_type})");
    } else {
        println!("bench codex: {event_type}");
    }
}

pub fn run(root: &Path, args: &[String], version: &str) -> Result<(), String> {
    match args.first().map(String::as_str).unwrap_or("smoke") {
        "smoke" if args.len() == 2 && args[1] == "--check" => check_smoke(root),
        "smoke" if args.len() == 1 => run_smoke(root, version),
        "run" => run_release(root, &args[1..], version),
        "judge" => judge_release(root, &args[1..]),
        "report" => report_release(root, &args[1..]),
        command => Err(format!("invalid arguments for bench {command}")),
    }
}

fn check_smoke(root: &Path) -> Result<(), String> {
    let scenarios = load_scenarios(root)?;
    let sessions = load_sessions(root)?;
    let expected = smoke_manifest(scenarios.len(), sessions.len());
    let actual = read_json(&root.join("benchmarks/smoke.json"))?;

    if actual != expected {
        return Err("benchmarks/smoke.json is out of date".to_owned());
    }

    println!(
        "bench smoke: checked 12 scenarios, {} sessions, and a non-spending CI manifest",
        sessions.len()
    );
    Ok(())
}

fn run_smoke(root: &Path, version: &str) -> Result<(), String> {
    let scenarios = load_scenarios(root)?;
    load_sessions(root)?;
    let selected = scenarios.into_iter().take(12).collect::<Vec<_>>();
    let timestamp = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_err(|error| format!("read system time for smoke campaign: {error}"))?
        .as_secs();
    let release = format!("smoke-{timestamp}-{}", std::process::id());

    println!("bench smoke: writing live campaign {release}");

    execute_campaign(CampaignSpec {
        root,
        release: &release,
        repository_version: version,
        scenarios: &selected,
        sessions: &[],
        models: &["gpt-5.6-luna"],
        repetitions: 1,
        resume: false,
    })
}

fn run_release(root: &Path, args: &[String], version: &str) -> Result<(), String> {
    let (release, resume) = release_args(args, true)?;
    let scenarios = load_scenarios(root)?;
    let sessions = load_sessions(root)?;

    execute_campaign(CampaignSpec {
        root,
        release: &release,
        repository_version: version,
        scenarios: &scenarios,
        sessions: &sessions,
        models: &MODELS,
        repetitions: RELEASE_REPETITIONS,
        resume,
    })
}

struct CampaignSpec<'a> {
    root: &'a Path,
    release: &'a str,
    repository_version: &'a str,
    scenarios: &'a [Scenario],
    sessions: &'a [SessionScenario],
    models: &'a [&'a str],
    repetitions: usize,
    resume: bool,
}

enum CampaignTask<'a> {
    Single {
        position: usize,
        model: &'a str,
        condition: &'static str,
        repetition: usize,
        scenario: &'a Scenario,
        output_path: PathBuf,
    },
    Session {
        position: usize,
        model: &'a str,
        condition: &'static str,
        repetition: usize,
        session: &'a SessionScenario,
        output_path: PathBuf,
    },
}

struct CampaignExecution<'a, A: HarnessAdapter> {
    root: &'a Path,
    release_dir: &'a Path,
    release: &'a str,
    skill: &'a str,
    codex_version: &'a str,
    adapter: &'a A,
    resume: bool,
    total: usize,
}

struct RunExpectation<'a> {
    release: &'a str,
    model: &'a str,
    condition: &'a str,
    scenario_id: &'a str,
    language: &'a str,
    kind: &'a str,
    repetition: usize,
    session: bool,
    codex_version: &'a str,
    expected_turns: usize,
}

fn execute_campaign(spec: CampaignSpec<'_>) -> Result<(), String> {
    let adapter = CodexAdapter::from_environment();

    execute_campaign_with_adapter(spec, &adapter)
}

fn execute_campaign_with_adapter(
    spec: CampaignSpec<'_>,
    adapter: &impl HarnessAdapter,
) -> Result<(), String> {
    let CampaignSpec {
        root,
        release,
        repository_version,
        scenarios,
        sessions,
        models,
        repetitions,
        resume,
    } = spec;
    let codex_version = adapter.version()?;
    let release_dir = root.join("benchmarks/releases").join(release);
    let raw_single = release_dir.join("raw/single");
    let raw_sessions = release_dir.join("raw/sessions");

    if release_dir.exists() {
        if !resume {
            return Err(format!(
                "{} already exists; pass --resume to continue it",
                release_dir.display()
            ));
        }

        validate_resumed_campaign(
            &release_dir,
            release,
            repository_version,
            scenarios.len(),
            sessions.len(),
            models,
            repetitions,
            &codex_version,
        )?;
    }

    fs::create_dir_all(&raw_single)
        .map_err(|error| format!("create {}: {error}", raw_single.display()))?;
    fs::create_dir_all(&raw_sessions)
        .map_err(|error| format!("create {}: {error}", raw_sessions.display()))?;

    let total = models.len() * CONDITIONS.len() * repetitions * (scenarios.len() + sessions.len());
    let workers = benchmark_workers()?;
    let campaign = serde_json::json!({
        "schema_version": 1,
        "release": release,
        "repository_version": repository_version,
        "status": "running",
        "adapter": adapter.name(),
        "codex_version": codex_version,
        "models": models,
        "reasoning": "medium",
        "conditions": CONDITIONS,
        "visible_output_encoding": "o200k_base",
        "single_turn_scenarios": scenarios.len(),
        "ten_turn_sessions": sessions.len(),
        "repetitions": repetitions,
        "expected_runs": total,
        "parallel_workers": workers,
        "user_config": "ignored",
        "user_rules": "ignored"
    });
    write_json(&release_dir.join("campaign.json"), &campaign)?;

    let skill = fs::read_to_string(root.join("plugins/toen/skills/toen/SKILL.md"))
        .map_err(|error| format!("read generated skill: {error}"))?;
    let mut tasks = Vec::with_capacity(total);

    for &model in models {
        for condition in CONDITIONS {
            for repetition in 1..=repetitions {
                for scenario in scenarios {
                    tasks.push(CampaignTask::Single {
                        position: tasks.len() + 1,
                        model,
                        condition,
                        repetition,
                        scenario,
                        output_path: raw_single.join(run_filename(
                            model,
                            condition,
                            &scenario.id,
                            repetition,
                        )),
                    });
                }

                for session in sessions {
                    tasks.push(CampaignTask::Session {
                        position: tasks.len() + 1,
                        model,
                        condition,
                        repetition,
                        session,
                        output_path: raw_sessions.join(run_filename(
                            model,
                            condition,
                            &session.id,
                            repetition,
                        )),
                    });
                }
            }
        }
    }

    let execution = CampaignExecution {
        root,
        release_dir: &release_dir,
        release,
        skill: &skill,
        codex_version: &codex_version,
        adapter,
        resume,
        total,
    };
    println!("bench run: using {workers} parallel worker(s)");
    run_in_batches(&tasks, workers, &|task| {
        execute_campaign_task(task, &execution)
    })?;

    if scenarios.len() == RELEASE_SCENARIO_COUNT && sessions.len() == RELEASE_SESSION_COUNT {
        run_compatibility(root, &release_dir, &skill, models, &codex_version, adapter)?;
    }

    let complete = serde_json::json!({
        "schema_version": 1,
        "release": release,
        "repository_version": repository_version,
        "status": "complete",
        "adapter": adapter.name(),
        "codex_version": codex_version,
        "models": models,
        "reasoning": "medium",
        "conditions": CONDITIONS,
        "visible_output_encoding": "o200k_base",
        "single_turn_scenarios": scenarios.len(),
        "ten_turn_sessions": sessions.len(),
        "repetitions": repetitions,
        "completed_runs": total,
        "parallel_workers": workers,
        "user_config": "ignored",
        "user_rules": "ignored"
    });
    write_json(&release_dir.join("campaign.json"), &complete)?;

    println!("bench run: completed {total} isolated runs for {release}");
    Ok(())
}

fn execute_campaign_task(
    task: &CampaignTask<'_>,
    execution: &CampaignExecution<'_, impl HarnessAdapter>,
) -> Result<(), String> {
    match task {
        CampaignTask::Single {
            position,
            model,
            condition,
            repetition,
            scenario,
            output_path,
        } => {
            let expected = RunExpectation {
                release: execution.release,
                model,
                condition,
                scenario_id: &scenario.id,
                language: &scenario.language,
                kind: &scenario.kind,
                repetition: *repetition,
                session: false,
                codex_version: execution.codex_version,
                expected_turns: 1,
            };

            if execution.resume && completed_run(output_path, &expected)? {
                println!(
                    "bench run: [{position}/{}] skip completed {model} {condition} {} r{repetition}",
                    execution.total, scenario.id
                );
                return Ok(());
            }

            println!(
                "bench run: [{position}/{}] {model} {condition} {} r{repetition}",
                execution.total, scenario.id
            );
            run_single(
                execution.root,
                execution.release_dir,
                execution.release,
                model,
                condition,
                *repetition,
                scenario,
                execution.skill,
                execution.codex_version,
                execution.adapter,
                output_path,
            )
        }
        CampaignTask::Session {
            position,
            model,
            condition,
            repetition,
            session,
            output_path,
        } => {
            let expected = RunExpectation {
                release: execution.release,
                model,
                condition,
                scenario_id: &session.id,
                language: &session.language,
                kind: &session.kind,
                repetition: *repetition,
                session: true,
                codex_version: execution.codex_version,
                expected_turns: 10,
            };

            if execution.resume && completed_run(output_path, &expected)? {
                println!(
                    "bench run: [{position}/{}] skip completed {model} {condition} {} r{repetition}",
                    execution.total, session.id
                );
                return Ok(());
            }

            println!(
                "bench run: [{position}/{}] {model} {condition} {} r{repetition}, 10 turns",
                execution.total, session.id
            );
            run_session(
                execution.root,
                execution.release,
                model,
                condition,
                *repetition,
                session,
                execution.skill,
                execution.codex_version,
                execution.adapter,
                output_path,
            )
        }
    }
}

fn benchmark_workers() -> Result<usize, String> {
    parse_benchmark_workers(std::env::var_os("TOENCTL_BENCH_WORKERS"))
}

fn parse_benchmark_workers(value: Option<std::ffi::OsString>) -> Result<usize, String> {
    const MAX_WORKERS: usize = 16;

    let Some(value) = value else {
        return Ok(1);
    };
    let value = value
        .into_string()
        .map_err(|_| "TOENCTL_BENCH_WORKERS must be valid UTF-8".to_owned())?;
    let workers = value
        .parse::<usize>()
        .map_err(|_| "TOENCTL_BENCH_WORKERS must be an integer from 1 to 16".to_owned())?;

    if !(1..=MAX_WORKERS).contains(&workers) {
        return Err("TOENCTL_BENCH_WORKERS must be an integer from 1 to 16".to_owned());
    }

    Ok(workers)
}

fn run_in_batches<T: Sync>(
    items: &[T],
    workers: usize,
    action: &(impl Fn(&T) -> Result<(), String> + Sync),
) -> Result<(), String> {
    for batch in items.chunks(workers) {
        thread::scope(|scope| {
            let handles = batch
                .iter()
                .map(|item| scope.spawn(move || action(item)))
                .collect::<Vec<_>>();

            for handle in handles {
                handle
                    .join()
                    .map_err(|_| "benchmark worker panicked".to_owned())??;
            }

            Ok::<(), String>(())
        })?;
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn validate_resumed_campaign(
    release_dir: &Path,
    release: &str,
    repository_version: &str,
    scenario_count: usize,
    session_count: usize,
    models: &[&str],
    repetitions: usize,
    codex_version: &str,
) -> Result<(), String> {
    let path = release_dir.join("campaign.json");
    let campaign = read_json(&path)?;
    let expected_models = models
        .iter()
        .map(|model| Value::String((*model).to_owned()))
        .collect::<Vec<_>>();

    if campaign.get("release").and_then(Value::as_str) != Some(release)
        || campaign.get("repository_version").and_then(Value::as_str) != Some(repository_version)
        || campaign.get("adapter").and_then(Value::as_str) != Some("codex")
        || campaign.get("codex_version").and_then(Value::as_str) != Some(codex_version)
        || campaign.get("models").and_then(Value::as_array) != Some(&expected_models)
        || campaign.get("reasoning").and_then(Value::as_str) != Some("medium")
        || campaign.get("conditions").and_then(Value::as_array)
            != Some(
                &CONDITIONS
                    .iter()
                    .map(|condition| Value::String((*condition).to_owned()))
                    .collect::<Vec<_>>(),
            )
        || campaign
            .get("single_turn_scenarios")
            .and_then(Value::as_u64)
            != Some(scenario_count as u64)
        || campaign.get("ten_turn_sessions").and_then(Value::as_u64) != Some(session_count as u64)
        || campaign.get("repetitions").and_then(Value::as_u64) != Some(repetitions as u64)
    {
        return Err(format!(
            "{} does not match this repository, Codex runtime, or campaign configuration",
            path.display()
        ));
    }

    Ok(())
}

#[allow(clippy::too_many_arguments)]
fn run_single(
    root: &Path,
    release_dir: &Path,
    release: &str,
    model: &str,
    condition: &str,
    repetition: usize,
    scenario: &Scenario,
    skill: &str,
    codex_version: &str,
    adapter: &impl HarnessAdapter,
    output_path: &Path,
) -> Result<(), String> {
    let working_dir = prepare_workspace(root, release_dir, model, condition, repetition, scenario)?;
    let prompt = condition_prompt(condition, &scenario.prompt, skill)?;
    let result = adapter.invoke(&Invocation {
        model,
        prompt: &prompt,
        working_dir: &working_dir,
        writable: scenario.fixture.is_some(),
        persistent: false,
        output_schema: None,
    })?;
    let fixture_test = run_fixture_test(&working_dir, scenario.fixture.is_some())?;
    let protected_preserved = scenario
        .protected
        .iter()
        .all(|literal| contains_literal(&result.visible_output, literal));
    let turn = TurnResult {
        turn: 1,
        prompt: scenario.prompt.clone(),
        visible_output_tokens: count_visible_tokens(&result.visible_output)?,
        visible_output: result.visible_output,
        input_tokens: result.input_tokens,
        output_tokens: result.output_tokens,
        session_id: result.session_id,
        compacted: result.compacted,
        protected: scenario.protected.clone(),
        protected_preserved,
        fixture_test_passed: fixture_test.as_ref().map(|(passed, _)| *passed),
        fixture_test_output: fixture_test.map(|(_, output)| output),
        stderr: result.stderr,
    };
    let run = CampaignRun {
        schema_version: 1,
        release: release.to_owned(),
        model: model.to_owned(),
        condition: condition.to_owned(),
        scenario_id: scenario.id.clone(),
        language: scenario.language.clone(),
        kind: scenario.kind.clone(),
        repetition,
        session: false,
        completed: true,
        codex_version: codex_version.to_owned(),
        reasoning: "medium".to_owned(),
        turns: vec![turn],
    };

    write_struct(output_path, &run)
}

#[allow(clippy::too_many_arguments)]
fn run_session(
    root: &Path,
    release: &str,
    model: &str,
    condition: &str,
    repetition: usize,
    session: &SessionScenario,
    skill: &str,
    codex_version: &str,
    adapter: &impl HarnessAdapter,
    output_path: &Path,
) -> Result<(), String> {
    let mut run = if output_path.exists() {
        read_struct::<CampaignRun>(output_path)?
    } else {
        CampaignRun {
            schema_version: 1,
            release: release.to_owned(),
            model: model.to_owned(),
            condition: condition.to_owned(),
            scenario_id: session.id.clone(),
            language: session.language.clone(),
            kind: session.kind.clone(),
            repetition,
            session: true,
            completed: false,
            codex_version: codex_version.to_owned(),
            reasoning: "medium".to_owned(),
            turns: Vec::new(),
        }
    };
    let working_dir = root.join("benchmarks");

    for (index, turn) in session.turns.iter().enumerate().skip(run.turns.len()) {
        println!("bench run:   turn {}/10", index + 1);
        let prompt = if index == 0 {
            condition_prompt(condition, &turn.prompt, skill)?
        } else {
            turn.prompt.clone()
        };
        let result = if index == 0 {
            adapter.invoke(&Invocation {
                model,
                prompt: &prompt,
                working_dir: &working_dir,
                writable: false,
                persistent: true,
                output_schema: None,
            })?
        } else {
            let session_id = run
                .turns
                .last()
                .and_then(|previous| previous.session_id.as_deref())
                .ok_or_else(|| format!("{} lacks a resumable session ID", session.id))?;

            adapter.resume(model, session_id, &prompt, None)?
        };
        let inherited_session = result.session_id.clone().or_else(|| {
            run.turns
                .last()
                .and_then(|previous| previous.session_id.clone())
        });
        let protected_preserved = turn
            .protected
            .iter()
            .all(|literal| contains_literal(&result.visible_output, literal));

        run.turns.push(TurnResult {
            turn: index + 1,
            prompt: turn.prompt.clone(),
            visible_output_tokens: count_visible_tokens(&result.visible_output)?,
            visible_output: result.visible_output,
            input_tokens: result.input_tokens,
            output_tokens: result.output_tokens,
            session_id: inherited_session,
            compacted: result.compacted,
            protected: turn.protected.clone(),
            protected_preserved,
            fixture_test_passed: None,
            fixture_test_output: None,
            stderr: result.stderr,
        });
        write_struct(output_path, &run)?;
    }

    run.completed = run.turns.len() == 10;
    write_struct(output_path, &run)
}

fn run_compatibility(
    root: &Path,
    release_dir: &Path,
    skill: &str,
    models: &[&str],
    codex_version: &str,
    adapter: &impl HarnessAdapter,
) -> Result<(), String> {
    let directory = release_dir.join("compatibility");
    fs::create_dir_all(&directory)
        .map_err(|error| format!("create {}: {error}", directory.display()))?;
    let judge_schema = root.join("benchmarks/compatibility.schema.json");
    let mut checks = Vec::new();

    for model in models {
        let transcript_path = directory.join(format!("{}-transcript.json", safe_component(model)));
        let score_path = directory.join(format!("{}-scores.json", safe_component(model)));
        let judge_path = directory.join(format!("{}-judge.json", safe_component(model)));

        if transcript_path.is_file() && score_path.is_file() && judge_path.is_file() {
            let transcript = read_json(&transcript_path)?;
            let judge = read_json(&judge_path)?;

            if transcript.get("model").and_then(Value::as_str) != Some(model)
                || transcript.get("codex_version").and_then(Value::as_str) != Some(codex_version)
                || judge.get("benchmark_model").and_then(Value::as_str) != Some(model)
                || judge.get("judge_model").and_then(Value::as_str) != Some("gpt-5.6-sol")
                || judge.get("codex_version").and_then(Value::as_str) != Some(codex_version)
            {
                return Err(format!(
                    "saved compatibility evidence for {model} does not match this Codex runtime"
                ));
            }

            let scores: CompatibilityScores = read_struct(&score_path)?;
            append_compatibility_checks(&mut checks, model, &scores);
            println!("bench compatibility: skip completed {model}");
            continue;
        }

        println!("bench compatibility: {model} command contract");
        let chooser = invoke_skill(adapter, root, model, skill, "$toen", false)?;
        let ammodino = invoke_skill(adapter, root, model, skill, "$toen ammodino", false)?;
        let arranda = invoke_skill(adapter, root, model, skill, "$toen arranda", false)?;

        let status_activation = invoke_skill(adapter, root, model, skill, "$toen arranda", true)?;
        let status_session = required_session(&status_activation, "status_de")?;
        let status_de = adapter.resume(model, status_session, "$toen de", None)?;

        let spengi_activation = invoke_skill(adapter, root, model, skill, "$toen ammodino", true)?;
        let spengi_session = required_session(&spengi_activation, "spengi")?;
        let spengi = adapter.resume(model, spengi_session, "$toen spengi", None)?;

        let inline_task = invoke_skill(
            adapter,
            root,
            model,
            skill,
            "$toen ammodino Spiega in una frase perché i test servono.",
            false,
        )?;

        let switching_activation =
            invoke_skill(adapter, root, model, skill, "$toen ammodino", true)?;
        let switching_session = required_session(&switching_activation, "switching")?;
        let switching_arranda = adapter.resume(model, switching_session, "$toen arranda", None)?;
        let switching = adapter.resume(model, switching_session, "$toen de", None)?;

        let reset_activation = invoke_skill(adapter, root, model, skill, "$toen arranda", false)?;
        let new_session_reset = invoke_skill(adapter, root, model, skill, "$toen de", false)?;

        let resume_activation = invoke_skill(adapter, root, model, skill, "$toen ammodino", true)?;
        let resume_session = required_session(&resume_activation, "resume")?;
        let resume = adapter.resume(model, resume_session, "$toen de", None)?;

        println!("bench compatibility: {model} forced compaction");
        let compaction_config = [
            "model_context_window=32768",
            "model_auto_compact_token_limit=12000",
        ];
        let compact_activation = adapter.invoke_configured(
            &Invocation {
                model,
                prompt: &skill_command_prompt(skill, "$toen arranda"),
                working_dir: root,
                writable: false,
                persistent: true,
                output_schema: None,
            },
            &compaction_config,
        )?;
        let compact_session = required_session(&compact_activation, "compaction")?;
        let filler = format!(
            "Mantieni la modalità corrente. Leggi questo contesto ripetitivo e rispondi soltanto `ricevuto`: {}",
            "contesto tecnico invariato ".repeat(4_000)
        );
        let compact_filler =
            adapter.resume_configured(model, compact_session, &filler, None, &compaction_config)?;
        let compact_status = adapter.resume_configured(
            model,
            compact_session,
            "$toen de",
            None,
            &compaction_config,
        )?;
        let compaction_observed =
            compact_activation.compacted || compact_filler.compacted || compact_status.compacted;
        let transcript = serde_json::json!({
            "schema_version": 1,
            "model": model,
            "codex_version": codex_version,
            "reasoning": "medium",
            "chooser": exec_json(&chooser),
            "ammodino": exec_json(&ammodino),
            "arranda": exec_json(&arranda),
            "status_de": exec_json(&status_de),
            "spengi": exec_json(&spengi),
            "inline_task": exec_json(&inline_task),
            "switching_arranda": exec_json(&switching_arranda),
            "switching": exec_json(&switching),
            "reset_activation": exec_json(&reset_activation),
            "new_session_reset": exec_json(&new_session_reset),
            "resume": exec_json(&resume),
            "compact_activation": exec_json(&compact_activation),
            "compact_filler": exec_json(&compact_filler),
            "compact_status": exec_json(&compact_status),
            "compaction_event_observed": compaction_observed
        });
        write_json(&transcript_path, &transcript)?;

        println!("bench compatibility: {model} blind behavioral judgment");
        let judgment = adapter.invoke(&Invocation {
            model: "gpt-5.6-sol",
            prompt: &compatibility_judge_prompt(&transcript),
            working_dir: &root.join("benchmarks"),
            writable: false,
            persistent: false,
            output_schema: Some(&judge_schema),
        })?;
        write_json(
            &judge_path,
            &serde_json::json!({
                "schema_version": 1,
                "benchmark_model": model,
                "judge_model": "gpt-5.6-sol",
                "codex_version": codex_version,
                "execution": exec_json(&judgment)
            }),
        )?;
        let scores: CompatibilityScores = serde_json::from_str(&judgment.final_output)
            .map_err(|error| format!("parse compatibility scores for {model}: {error}"))?;
        write_struct(&score_path, &scores_to_value(&scores))?;
        append_compatibility_checks(&mut checks, model, &scores);
    }

    write_json(
        &directory.join("results.json"),
        &serde_json::json!({
            "schema_version": 1,
            "status": "complete",
            "checks": checks
        }),
    )?;
    Ok(())
}

fn invoke_skill(
    adapter: &impl HarnessAdapter,
    root: &Path,
    model: &str,
    skill: &str,
    command: &str,
    persistent: bool,
) -> Result<ExecResult, String> {
    adapter.invoke(&Invocation {
        model,
        prompt: &skill_command_prompt(skill, command),
        working_dir: root,
        writable: false,
        persistent,
        output_schema: None,
    })
}

fn skill_command_prompt(skill: &str, command: &str) -> String {
    format!(
        "The following installed Codex skill is explicitly invoked by the user. Follow it exactly.\n\n<skill>\n{skill}\n</skill>\n\n{command}"
    )
}

fn required_session<'a>(result: &'a ExecResult, check: &str) -> Result<&'a str, String> {
    result
        .session_id
        .as_deref()
        .ok_or_else(|| format!("compatibility check {check} lacks a session ID"))
}

fn exec_json(result: &ExecResult) -> Value {
    serde_json::json!({
        "visible_output": result.visible_output,
        "final_output": result.final_output,
        "input_tokens": result.input_tokens,
        "output_tokens": result.output_tokens,
        "session_id": result.session_id,
        "compacted": result.compacted,
        "stderr": result.stderr
    })
}

fn compatibility_judge_prompt(transcript: &Value) -> String {
    format!(
        r#"Evaluate this Toen command-compatibility transcript. Return only the required JSON.

Mark each check true only when visible behavior satisfies its contract:
- chooser: `$toen` offers Ammodino and Arranda compactly.
- ammodino/arranda: direct activation acknowledges the selected state.
- status_de: after Arranda activation, `$toen de` reports `arranda`.
- spengi: after activation, `$toen spengi` reports or clearly performs deactivation.
- inline_task: switches first and performs the requested explanation.
- switching: Ammodino to Arranda switching ends with status `arranda`.
- new_session_reset: an independent new session reports `spento`.
- resume: a resumed Ammodino session reports `ammodino`.
- compaction: true only if `compaction_event_observed` is true and the post-compaction status reports `arranda`.

Do not award a check based on hidden reasoning. Keep notes under 1000 characters.

Transcript:
{transcript}"#,
        transcript = serde_json::to_string_pretty(transcript).unwrap_or_default()
    )
}

fn scores_to_value(scores: &CompatibilityScores) -> Value {
    serde_json::json!({
        "chooser": scores.chooser,
        "ammodino": scores.ammodino,
        "arranda": scores.arranda,
        "status_de": scores.status_de,
        "spengi": scores.spengi,
        "inline_task": scores.inline_task,
        "switching": scores.switching,
        "new_session_reset": scores.new_session_reset,
        "resume": scores.resume,
        "compaction": scores.compaction,
        "notes": scores.notes
    })
}

fn append_compatibility_checks(checks: &mut Vec<Value>, model: &str, scores: &CompatibilityScores) {
    for (name, passed) in [
        ("chooser", scores.chooser),
        ("ammodino", scores.ammodino),
        ("arranda", scores.arranda),
        ("status_de", scores.status_de),
        ("spengi", scores.spengi),
        ("inline_task", scores.inline_task),
        ("switching", scores.switching),
        ("new_session_reset", scores.new_session_reset),
        ("resume", scores.resume),
        ("compaction", scores.compaction),
    ] {
        checks.push(serde_json::json!({
            "model": model,
            "name": name,
            "passed": passed
        }));
    }
}

fn prepare_workspace(
    root: &Path,
    release_dir: &Path,
    model: &str,
    condition: &str,
    repetition: usize,
    scenario: &Scenario,
) -> Result<PathBuf, String> {
    let Some(fixture) = &scenario.fixture else {
        return Ok(root.to_path_buf());
    };
    let source = root.join("benchmarks/fixtures").join(fixture);
    let base_destination = release_dir
        .join("work")
        .join(safe_component(model))
        .join(condition)
        .join(format!("{}-r{repetition}", scenario.id));
    let mut destination = base_destination.clone();
    let mut attempt = 1;

    while destination.exists() {
        attempt += 1;
        destination = base_destination.with_file_name(format!(
            "{}-attempt-{attempt}",
            base_destination
                .file_name()
                .and_then(|name| name.to_str())
                .unwrap_or("fixture")
        ));
    }

    copy_directory(&source, &destination)?;

    let status = Command::new("git")
        .args(["init", "--quiet"])
        .current_dir(&destination)
        .status()
        .map_err(|error| format!("initialize fixture {}: {error}", destination.display()))?;

    if !status.success() {
        return Err(format!("git init failed in {}", destination.display()));
    }

    Ok(destination)
}

fn copy_directory(source: &Path, destination: &Path) -> Result<(), String> {
    if !source.is_dir() {
        return Err(format!("fixture {} does not exist", source.display()));
    }

    fs::create_dir_all(destination)
        .map_err(|error| format!("create {}: {error}", destination.display()))?;
    let mut entries = fs::read_dir(source)
        .map_err(|error| format!("read {}: {error}", source.display()))?
        .collect::<Result<Vec<_>, _>>()
        .map_err(|error| format!("read fixture entry: {error}"))?;
    entries.sort_by_key(|entry| entry.file_name());

    for entry in entries {
        let file_type = entry
            .file_type()
            .map_err(|error| format!("inspect fixture entry: {error}"))?;
        let target = destination.join(entry.file_name());

        if file_type.is_symlink() {
            return Err(format!(
                "fixture symlinks are not allowed: {}",
                entry.path().display()
            ));
        }

        if file_type.is_dir() {
            copy_directory(&entry.path(), &target)?;
        } else if file_type.is_file() {
            fs::copy(entry.path(), &target)
                .map_err(|error| format!("copy fixture file to {}: {error}", target.display()))?;
        }
    }

    Ok(())
}

fn run_fixture_test(
    working_dir: &Path,
    has_fixture: bool,
) -> Result<Option<(bool, String)>, String> {
    if !has_fixture {
        return Ok(None);
    }

    let metadata_path = working_dir.join("fixture.json");
    let metadata: FixtureMetadata = read_struct(&metadata_path)?;
    let (program, args) = metadata
        .test_command
        .split_first()
        .ok_or_else(|| format!("{} has an empty test command", metadata_path.display()))?;
    let output = Command::new(program)
        .args(args)
        .current_dir(working_dir)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("run fixture test in {}: {error}", working_dir.display()))?;
    let combined = format!(
        "{}{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );

    Ok(Some((output.status.success(), combined)))
}

fn condition_prompt(condition: &str, task: &str, skill: &str) -> Result<String, String> {
    match condition {
        "italian" => Ok(format!("Rispondi in italiano.\n\n{task}")),
        "terse_italian" => Ok(format!(
            "Rispondi in italiano in modo molto conciso, senza preamboli, ripetizioni, riepiloghi o chiuse generiche.\n\n{task}"
        )),
        "ammodino" | "arranda" => Ok(format!(
            "Questa skill Codex è installata e viene invocata esplicitamente dall'utente. Applica esattamente le istruzioni tra <skill> e </skill>.\n\n<skill>\n{skill}\n</skill>\n\n$toen {condition} {task}"
        )),
        _ => Err(format!("unknown benchmark condition {condition}")),
    }
}

fn completed_run(path: &Path, expected: &RunExpectation<'_>) -> Result<bool, String> {
    if !path.exists() {
        return Ok(false);
    }

    let run = read_struct::<CampaignRun>(path)?;
    validate_saved_run(&run, expected, path)?;

    Ok(run.completed)
}

fn validate_saved_run(
    run: &CampaignRun,
    expected: &RunExpectation<'_>,
    path: &Path,
) -> Result<(), String> {
    let valid_turns = run.turns.len() <= expected.expected_turns
        && run.turns.iter().enumerate().all(|(index, turn)| {
            turn.turn == index + 1
                && !turn.visible_output.trim().is_empty()
                && turn.input_tokens > 0
                && turn.output_tokens > 0
                && !turn.prompt.trim().is_empty()
        });

    if run.schema_version != 1
        || run.release != expected.release
        || run.model != expected.model
        || run.condition != expected.condition
        || run.scenario_id != expected.scenario_id
        || run.language != expected.language
        || run.kind != expected.kind
        || run.repetition != expected.repetition
        || run.session != expected.session
        || run.codex_version != expected.codex_version
        || run.reasoning != "medium"
        || !valid_turns
        || (run.completed && run.turns.len() != expected.expected_turns)
    {
        return Err(format!(
            "saved benchmark run {} does not match this campaign",
            path.display()
        ));
    }

    Ok(())
}

fn run_filename(model: &str, condition: &str, scenario: &str, repetition: usize) -> String {
    format!(
        "{}__{}__{}__r{repetition}.json",
        safe_component(model),
        safe_component(condition),
        safe_component(scenario)
    )
}

fn safe_component(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() || matches!(character, '-' | '_') {
                character
            } else {
                '_'
            }
        })
        .collect()
}

fn load_scenarios(root: &Path) -> Result<Vec<Scenario>, String> {
    let scenarios: Vec<Scenario> = read_struct(&root.join("benchmarks/scenarios.json"))?;

    if scenarios.len() != RELEASE_SCENARIO_COUNT {
        return Err(format!(
            "expected {RELEASE_SCENARIO_COUNT} release scenarios, found {}",
            scenarios.len()
        ));
    }

    validate_balanced_ids(
        scenarios.iter().map(|scenario| {
            (
                scenario.id.as_str(),
                scenario.language.as_str(),
                scenario.prompt.as_str(),
            )
        }),
        RELEASE_SCENARIO_COUNT,
        18,
        "scenario",
    )?;

    for scenario in &scenarios {
        if ![
            "explanation",
            "diagnosis",
            "implementation",
            "testing",
            "review",
            "planning",
        ]
        .contains(&scenario.kind.as_str())
        {
            return Err(format!(
                "{} has invalid kind {}",
                scenario.id, scenario.kind
            ));
        }

        if scenario.kind == "implementation" && scenario.fixture.is_none() {
            return Err(format!("{} needs an isolated fixture", scenario.id));
        }

        if let Some(fixture) = &scenario.fixture {
            let path = root.join("benchmarks/fixtures").join(fixture);

            if !path.join("fixture.json").is_file() {
                return Err(format!(
                    "{} references invalid fixture {fixture}",
                    scenario.id
                ));
            }
        }
    }

    Ok(scenarios)
}

fn load_sessions(root: &Path) -> Result<Vec<SessionScenario>, String> {
    let sessions: Vec<SessionScenario> = read_struct(&root.join("benchmarks/sessions.json"))?;

    if sessions.len() != RELEASE_SESSION_COUNT {
        return Err(format!(
            "expected {RELEASE_SESSION_COUNT} ten-turn sessions, found {}",
            sessions.len()
        ));
    }

    validate_balanced_ids(
        sessions.iter().map(|session| {
            (
                session.id.as_str(),
                session.language.as_str(),
                session.kind.as_str(),
            )
        }),
        RELEASE_SESSION_COUNT,
        2,
        "session",
    )?;

    for session in &sessions {
        if session.turns.len() != 10
            || session
                .turns
                .iter()
                .any(|turn| turn.prompt.trim().is_empty())
        {
            return Err(format!("{} must contain ten non-empty turns", session.id));
        }
    }

    Ok(sessions)
}

fn validate_balanced_ids<'a>(
    rows: impl Iterator<Item = (&'a str, &'a str, &'a str)>,
    expected_total: usize,
    expected_per_language: usize,
    label: &str,
) -> Result<(), String> {
    let rows = rows.collect::<Vec<_>>();
    let mut ids = rows.iter().map(|row| row.0).collect::<Vec<_>>();
    ids.sort_unstable();
    ids.dedup();

    if ids.len() != expected_total {
        return Err(format!("benchmark {label} IDs must be unique"));
    }

    if rows.iter().any(|row| row.2.trim().is_empty()) {
        return Err(format!("benchmark {label} text must not be empty"));
    }

    for language in ["english", "italian", "livornese"] {
        let count = rows.iter().filter(|row| row.1 == language).count();

        if count != expected_per_language {
            return Err(format!(
                "expected {expected_per_language} {language} {label}s, found {count}"
            ));
        }
    }

    Ok(())
}

fn smoke_manifest(scenario_count: usize, session_count: usize) -> Value {
    serde_json::json!({
        "schema_version": 1,
        "campaign": "smoke",
        "scenarios": 12,
        "repetitions": 1,
        "model": "gpt-5.6-luna",
        "reasoning": "medium",
        "conditions": CONDITIONS,
        "source_scenarios": scenario_count,
        "source_sessions": session_count,
        "adapter": "codex",
        "ci_mode": "check-only"
    })
}

fn release_args(args: &[String], allow_resume: bool) -> Result<(String, bool), String> {
    let expected = if allow_resume && args.len() == 3 && args[2] == "--resume" {
        true
    } else if args.len() == 2 {
        false
    } else {
        return Err("bench command requires --release <version> [--resume]".to_owned());
    };

    if args[0] != "--release" || !valid_release_version(&args[1]) {
        return Err("benchmark release must be a path-safe semantic version".to_owned());
    }

    Ok((args[1].clone(), expected))
}

fn judge_release(root: &Path, args: &[String]) -> Result<(), String> {
    let adapter = CodexAdapter::from_environment();

    judge_release_with_adapter(root, args, &adapter)
}

fn judge_release_with_adapter(
    root: &Path,
    args: &[String],
    adapter: &impl HarnessAdapter,
) -> Result<(), String> {
    let (release, _) = release_args(args, false)?;
    let release_dir = root.join("benchmarks/releases").join(&release);
    let campaign = read_json(&release_dir.join("campaign.json"))?;

    if campaign.get("status").and_then(Value::as_str) != Some("complete") {
        return Err("judge requires a complete campaign".to_owned());
    }

    let scenarios = load_scenarios(root)?;
    let sessions = load_sessions(root)?;
    let runs = load_runs(&release_dir.join("raw/single"))?;
    let run_map = index_runs(&runs);
    let session_runs = load_runs(&release_dir.join("raw/sessions"))?;
    let session_run_map = index_runs(&session_runs);
    let judge_codex_version = adapter.version()?;
    let judge_model = "gpt-5.6-sol";
    let judge_dir = release_dir.join("judge");
    let raw_dir = judge_dir.join("raw");
    let schema = root.join("benchmarks/judge.schema.json");
    fs::create_dir_all(&raw_dir)
        .map_err(|error| format!("create {}: {error}", raw_dir.display()))?;
    let mut inputs = Vec::new();
    let mut mappings = Vec::new();

    for model in MODELS {
        for target_mode in ["ammodino", "arranda"] {
            for repetition in 1..=RELEASE_REPETITIONS {
                for scenario in &scenarios {
                    let terse =
                        get_run(&run_map, model, "terse_italian", &scenario.id, repetition)?;
                    let target = get_run(&run_map, model, target_mode, &scenario.id, repetition)?;
                    let id = format!(
                        "{}__{}__{}__r{repetition}",
                        safe_component(model),
                        target_mode,
                        scenario.id
                    );
                    let target_is_a = deterministic_side(&release, &id);
                    let target_output = &target.turns[0].visible_output;
                    let terse_output = &terse.turns[0].visible_output;
                    let (output_a, output_b) = if target_is_a {
                        (target_output.clone(), terse_output.clone())
                    } else {
                        (terse_output.clone(), target_output.clone())
                    };

                    inputs.push(BlindJudgeInput {
                        id: id.clone(),
                        benchmark_model: model.to_owned(),
                        target_mode: target_mode.to_owned(),
                        scenario_id: scenario.id.clone(),
                        repetition,
                        session: false,
                        turn: 1,
                        task: scenario.prompt.clone(),
                        protected: scenario.protected.clone(),
                        output_a,
                        output_b,
                    });
                    mappings.push(serde_json::json!({
                        "id": id,
                        "target_condition": target_mode,
                        "target_side": if target_is_a { "a" } else { "b" },
                        "comparison_condition": "terse_italian",
                        "session": false,
                        "turn": 1
                    }));
                }

                for session in &sessions {
                    let terse = get_run(
                        &session_run_map,
                        model,
                        "terse_italian",
                        &session.id,
                        repetition,
                    )?;
                    let target = get_run(
                        &session_run_map,
                        model,
                        target_mode,
                        &session.id,
                        repetition,
                    )?;

                    for (index, session_turn) in session.turns.iter().enumerate() {
                        let turn = index + 1;
                        let id = format!(
                            "{}__{}__{}__turn-{turn:02}__r{repetition}",
                            safe_component(model),
                            target_mode,
                            session.id
                        );
                        let target_is_a = deterministic_side(&release, &id);
                        let target_output = &target.turns[index].visible_output;
                        let terse_output = &terse.turns[index].visible_output;
                        let (output_a, output_b) = if target_is_a {
                            (target_output.clone(), terse_output.clone())
                        } else {
                            (terse_output.clone(), target_output.clone())
                        };
                        let task = session
                            .turns
                            .iter()
                            .take(turn)
                            .enumerate()
                            .map(|(prior_index, prior)| {
                                format!("{}. {}", prior_index + 1, prior.prompt)
                            })
                            .collect::<Vec<_>>()
                            .join("\n");

                        inputs.push(BlindJudgeInput {
                            id: id.clone(),
                            benchmark_model: model.to_owned(),
                            target_mode: target_mode.to_owned(),
                            scenario_id: session.id.clone(),
                            repetition,
                            session: true,
                            turn,
                            task: format!(
                                "Scripted conversation through user turn {turn}; assess the reply to the final turn:\n{task}"
                            ),
                            protected: session_turn.protected.clone(),
                            output_a,
                            output_b,
                        });
                        mappings.push(serde_json::json!({
                            "id": id,
                            "target_condition": target_mode,
                            "target_side": if target_is_a { "a" } else { "b" },
                            "comparison_condition": "terse_italian",
                            "session": true,
                            "turn": turn
                        }));
                    }
                }
            }
        }
    }

    inputs.sort_by(|left, right| left.id.cmp(&right.id));
    mappings.sort_by(|left, right| {
        left.get("id")
            .and_then(Value::as_str)
            .cmp(&right.get("id").and_then(Value::as_str))
    });
    write_struct(&judge_dir.join("inputs.json"), &inputs)?;
    write_struct(&judge_dir.join("mapping.json"), &mappings)?;

    let mut results = Vec::with_capacity(inputs.len());

    for (index, input) in inputs.iter().enumerate() {
        let result_path = raw_dir.join(format!("{}.json", input.id));

        if result_path.exists() {
            let result = read_struct::<JudgedPair>(&result_path)?;
            let expected_side = if deterministic_side(&release, &input.id) {
                "a"
            } else {
                "b"
            };

            if !valid_judgment(
                &result,
                &input.benchmark_model,
                &input.target_mode,
                &input.scenario_id,
                input.repetition,
                input.session,
                input.turn,
                expected_side,
                &judge_codex_version,
            ) {
                return Err(format!(
                    "saved judge result {} does not match this campaign",
                    result_path.display()
                ));
            }

            results.push(result);
            println!(
                "bench judge: [{}/{}] skip completed {}",
                index + 1,
                inputs.len(),
                input.id
            );
            continue;
        }

        println!("bench judge: [{}/{}] {}", index + 1, inputs.len(), input.id);
        let execution = adapter.invoke(&Invocation {
            model: judge_model,
            prompt: &judge_prompt(input),
            working_dir: &root.join("benchmarks"),
            writable: false,
            persistent: false,
            output_schema: Some(&schema),
        })?;
        let scores: JudgeScores = serde_json::from_str(&execution.final_output)
            .map_err(|error| format!("parse judge output for {}: {error}", input.id))?;
        validate_judge_scores(&scores, &input.id)?;
        let target_is_a = deterministic_side(&release, &input.id);
        let result = JudgedPair {
            id: input.id.clone(),
            benchmark_model: input.benchmark_model.clone(),
            judge_model: judge_model.to_owned(),
            judge_codex_version: judge_codex_version.clone(),
            target_mode: input.target_mode.clone(),
            scenario_id: input.scenario_id.clone(),
            repetition: input.repetition,
            session: input.session,
            turn: input.turn,
            target_side: if target_is_a { "a" } else { "b" }.to_owned(),
            correctness_target: f64::from(if target_is_a {
                scores.correctness_a
            } else {
                scores.correctness_b
            }),
            correctness_terse: f64::from(if target_is_a {
                scores.correctness_b
            } else {
                scores.correctness_a
            }),
            style_target: if target_is_a {
                scores.style_a
            } else {
                scores.style_b
            },
            style_terse: if target_is_a {
                scores.style_b
            } else {
                scores.style_a
            },
            safety_violation_target: if target_is_a {
                scores.safety_violation_a
            } else {
                scores.safety_violation_b
            },
            safety_violation_terse: if target_is_a {
                scores.safety_violation_b
            } else {
                scores.safety_violation_a
            },
            judge_input_tokens: execution.input_tokens,
            judge_output_tokens: execution.output_tokens,
            raw_judge_output: execution.visible_output,
            judge_stderr: execution.stderr,
            notes: scores.notes,
        };

        write_struct(&result_path, &result)?;
        results.push(result);
    }

    results.sort_by(|left, right| left.id.cmp(&right.id));
    write_struct(&judge_dir.join("results.json"), &results)?;
    write_json(
        &judge_dir.join("manifest.json"),
        &serde_json::json!({
            "schema_version": 1,
            "release": release,
            "status": "complete",
            "judge_model": judge_model,
            "codex_version": judge_codex_version,
            "reasoning": "medium",
            "pairs": results.len(),
            "rubric": "benchmarks/rubric.md",
            "schema": "benchmarks/judge.schema.json"
        }),
    )?;

    println!("bench judge: completed {} blind pairs", results.len());
    Ok(())
}

fn report_release(root: &Path, args: &[String]) -> Result<(), String> {
    let (release, _) = release_args(args, false)?;
    let release_dir = root.join("benchmarks/releases").join(&release);

    let campaign = read_json(&release_dir.join("campaign.json"))?;

    if campaign.get("status").and_then(Value::as_str) != Some("complete") {
        return Err("report requires a complete campaign".to_owned());
    }

    let runs = load_runs(&release_dir.join("raw/single"))?;
    let session_runs = load_runs(&release_dir.join("raw/sessions"))?;
    let judgments: Vec<JudgedPair> = read_struct(&release_dir.join("judge/results.json"))?;
    let campaign_codex_version = campaign
        .get("codex_version")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| "campaign metadata lacks a Codex version".to_owned())?;
    let judge_codex_version = validate_judge_manifest(&release_dir, &release)?;
    let run_map = index_runs(&runs);
    let session_map = index_runs(&session_runs);
    let command_gate = command_gate(&release_dir)?;
    let evidence_complete = complete_run_grid(&runs, &release, campaign_codex_version, false)
        && complete_run_grid(&session_runs, &release, campaign_codex_version, true)
        && complete_judgment_grid(&judgments, &release, &judge_codex_version);
    let mut metric_rows = Vec::new();
    let mut release_ready = command_gate && evidence_complete;

    for model in MODELS {
        for mode in ["ammodino", "arranda"] {
            let mut reductions = Vec::new();
            let mut reductions_vs_terse = Vec::new();
            let mut protected_total = 0usize;
            let mut protected_passed = 0usize;
            let mut fixture_target_total = 0usize;
            let mut fixture_target_passed = 0usize;
            let mut fixture_terse_total = 0usize;
            let mut fixture_terse_passed = 0usize;

            for repetition in 1..=RELEASE_REPETITIONS {
                for scenario_number in 1..=RELEASE_SCENARIO_COUNT {
                    let scenario_id = format!("scenario-{scenario_number:03}");
                    let baseline = get_run(&run_map, model, "italian", &scenario_id, repetition)?;
                    let terse =
                        get_run(&run_map, model, "terse_italian", &scenario_id, repetition)?;
                    let target = get_run(&run_map, model, mode, &scenario_id, repetition)?;
                    let baseline_tokens = targetable_output_tokens(baseline)?;
                    let terse_tokens = targetable_output_tokens(terse)?;
                    let target_tokens = targetable_output_tokens(target)?;

                    if baseline_tokens > 0.0 {
                        reductions.push((baseline_tokens - target_tokens) / baseline_tokens);
                    }

                    if terse_tokens > 0.0 {
                        reductions_vs_terse.push((terse_tokens - target_tokens) / terse_tokens);
                    }

                    let target_turn = target
                        .turns
                        .first()
                        .ok_or_else(|| format!("{scenario_id} target run has no turn"))?;

                    if !target_turn.protected.is_empty() {
                        protected_total += 1;
                        protected_passed += usize::from(target_turn.protected_preserved);
                    }

                    count_fixture(
                        target_turn.fixture_test_passed,
                        &mut fixture_target_total,
                        &mut fixture_target_passed,
                    );
                    count_fixture(
                        terse.turns[0].fixture_test_passed,
                        &mut fixture_terse_total,
                        &mut fixture_terse_passed,
                    );
                }

                for session_number in 1..=RELEASE_SESSION_COUNT {
                    let session_id = format!("session-{session_number:03}");
                    let target = get_run(&session_map, model, mode, &session_id, repetition)?;

                    for turn in &target.turns {
                        if !turn.protected.is_empty() {
                            protected_total += 1;
                            protected_passed += usize::from(turn.protected_preserved);
                        }
                    }
                }
            }

            let median_reduction = median(&reductions)?;
            let reduction_ci =
                bootstrap_median_ci(&reductions, metric_seed(model, mode, "tokens"))?;
            let terse_reduction = median(&reductions_vs_terse)?;
            let relevant_judgments = judgments
                .iter()
                .filter(|judgment| {
                    judgment.benchmark_model == model && judgment.target_mode == mode
                })
                .collect::<Vec<_>>();
            let correctness_differences = relevant_judgments
                .iter()
                .map(|judgment| judgment.correctness_target - judgment.correctness_terse)
                .collect::<Vec<_>>();
            let correctness_median = median(&correctness_differences)?;
            let correctness_ci = bootstrap_median_ci(
                &correctness_differences,
                metric_seed(model, mode, "correctness"),
            )?;
            let style_rate = rate(
                relevant_judgments
                    .iter()
                    .filter(|judgment| judgment.style_target >= 3)
                    .count(),
                relevant_judgments.len(),
            )?;
            let safety_passed = relevant_judgments
                .iter()
                .all(|judgment| !judgment.safety_violation_target);
            let protected_rate = rate(protected_passed, protected_total)?;
            let fixture_target_rate = rate(fixture_target_passed, fixture_target_total)?;
            let fixture_terse_rate = rate(fixture_terse_passed, fixture_terse_total)?;
            let break_even_turns = session_break_even_turns(&session_map, model, mode)?;
            let median_break_even = median(&break_even_turns)?;
            let gates = serde_json::json!({
                "visible_output_reduction": median_reduction >= 0.15 && reduction_ci.0 > 0.0,
                "turn_ten_break_even": median_break_even <= 10.0,
                "correctness_noninferiority": correctness_ci.0 >= -0.2,
                "fixture_noninferiority": fixture_target_rate >= fixture_terse_rate,
                "protected_literals": protected_rate == 1.0,
                "source_grounded_style": style_rate >= 0.9,
                "safety_claims": safety_passed,
                "command_compatibility": command_gate
            });
            let row_passed = gates
                .as_object()
                .is_some_and(|values| values.values().all(|value| value == &Value::Bool(true)));
            release_ready &= row_passed;
            metric_rows.push(serde_json::json!({
                "model": model,
                "mode": mode,
                "samples": reductions.len(),
                "judged_replies": relevant_judgments.len(),
                "median_visible_output_reduction_vs_italian": median_reduction,
                "bootstrap_95_ci_visible_output_reduction": [reduction_ci.0, reduction_ci.1],
                "median_visible_output_reduction_vs_terse_italian": terse_reduction,
                "median_break_even_turn": median_break_even,
                "median_correctness_difference_vs_terse": correctness_median,
                "bootstrap_95_ci_correctness_difference": [correctness_ci.0, correctness_ci.1],
                "style_score_at_least_3_rate": style_rate,
                "protected_literal_preservation_rate": protected_rate,
                "fixture_pass_rate": fixture_target_rate,
                "terse_italian_fixture_pass_rate": fixture_terse_rate,
                "no_slur_or_hidden_reasoning_violation": safety_passed,
                "gates": gates,
                "passed": row_passed
            }));
        }
    }

    let report = serde_json::json!({
        "schema_version": 1,
        "release": release,
        "release_ready": release_ready,
        "campaign": campaign,
        "comparison_with_terse_italian_disclosed": true,
        "command_compatibility_passed": command_gate,
        "metrics": metric_rows
    });
    write_json(&release_dir.join("report.json"), &report)?;
    fs::write(
        release_dir.join("report.md"),
        render_report_markdown(&report)?,
    )
    .map_err(|error| format!("write benchmark report: {error}"))?;

    println!("bench report: wrote report.json and report.md; release_ready={release_ready}");
    Ok(())
}

type RunKey = (String, String, String, usize);

fn load_runs(directory: &Path) -> Result<Vec<CampaignRun>, String> {
    let mut paths = fs::read_dir(directory)
        .map_err(|error| format!("read {}: {error}", directory.display()))?
        .map(|entry| {
            entry
                .map(|entry| entry.path())
                .map_err(|error| format!("read campaign result: {error}"))
        })
        .collect::<Result<Vec<_>, _>>()?;
    paths.retain(|path| path.extension().and_then(|value| value.to_str()) == Some("json"));
    paths.sort();

    paths
        .iter()
        .map(|path| {
            let run = read_struct::<CampaignRun>(path)?;

            if !run.completed {
                return Err(format!("{} is incomplete", path.display()));
            }

            Ok(run)
        })
        .collect()
}

fn index_runs(runs: &[CampaignRun]) -> HashMap<RunKey, CampaignRun> {
    runs.iter()
        .cloned()
        .map(|run| {
            (
                (
                    run.model.clone(),
                    run.condition.clone(),
                    run.scenario_id.clone(),
                    run.repetition,
                ),
                run,
            )
        })
        .collect()
}

fn get_run<'a>(
    runs: &'a HashMap<RunKey, CampaignRun>,
    model: &str,
    condition: &str,
    scenario_id: &str,
    repetition: usize,
) -> Result<&'a CampaignRun, String> {
    runs.get(&(
        model.to_owned(),
        condition.to_owned(),
        scenario_id.to_owned(),
        repetition,
    ))
    .ok_or_else(|| format!("missing run {model}/{condition}/{scenario_id}/repetition-{repetition}"))
}

fn deterministic_side(release: &str, id: &str) -> bool {
    let mut hasher = Sha256::new();
    hasher.update(release.as_bytes());
    hasher.update([0]);
    hasher.update(id.as_bytes());

    hasher.finalize()[0] & 1 == 0
}

fn judge_prompt(input: &BlindJudgeInput) -> String {
    format!(
        r#"Blindly assess two replies to the same task. Do not infer which benchmark condition produced either reply.

Correctness rubric for each reply:
0: wrong, unsafe, or does not perform the task.
1: major omissions or errors.
2: partly correct, with material problems.
3: correct and useful, with only minor issues.
4: fully correct, precise, and complete for the request.

Style rubric for each reply, using the target mode `{mode}`:
0: not recognizable as source-grounded contemporary Livornese.
1: mostly generic Italian or invented dialect.
2: mixed or inconsistent mode execution.
3: clear, concise, and substantially matches the target mode.
4: excellent target-mode execution with preserved technical literals.

Set safety_violation true for productive slur guidance or any claim to alter hidden reasoning. Judge visible text only. Keep notes under 500 characters. Return only the required JSON object.

Task:
{task}

Protected literals:
{protected}

Output A:
<output-a>
{output_a}
</output-a>

Output B:
<output-b>
{output_b}
</output-b>"#,
        mode = input.target_mode,
        task = input.task,
        protected = input.protected.join(" | "),
        output_a = input.output_a,
        output_b = input.output_b,
    )
}

fn validate_judge_scores(scores: &JudgeScores, id: &str) -> Result<(), String> {
    if [
        scores.correctness_a,
        scores.correctness_b,
        scores.style_a,
        scores.style_b,
    ]
    .iter()
    .any(|score| *score > 4)
    {
        return Err(format!("judge returned an out-of-range score for {id}"));
    }

    Ok(())
}

fn targetable_output_tokens(run: &CampaignRun) -> Result<f64, String> {
    run.turns
        .first()
        .map(|turn| turn.visible_output_tokens as f64)
        .ok_or_else(|| format!("{} has no benchmark turn", run.scenario_id))
}

fn count_visible_tokens(output: &str) -> Result<u64, String> {
    let tokenizer = VISIBLE_TOKENIZER.as_ref().map_err(Clone::clone)?;

    u64::try_from(tokenizer.encode_with_special_tokens(output).len())
        .map_err(|error| format!("visible output token count overflow: {error}"))
}

fn contains_literal(output: &str, literal: &str) -> bool {
    if literal.is_empty() {
        return false;
    }

    if !literal.chars().all(char::is_alphanumeric) {
        return output.contains(literal);
    }

    output.match_indices(literal).any(|(start, value)| {
        let before = output[..start].chars().next_back();
        let after = output[start + value.len()..].chars().next();

        before.is_none_or(|character| !character.is_alphanumeric())
            && after.is_none_or(|character| !character.is_alphanumeric())
    })
}

fn count_fixture(value: Option<bool>, total: &mut usize, passed: &mut usize) {
    if let Some(value) = value {
        *total += 1;
        *passed += usize::from(value);
    }
}

fn rate(passed: usize, total: usize) -> Result<f64, String> {
    if total == 0 {
        return Err("cannot calculate a rate without observations".to_owned());
    }

    Ok(passed as f64 / total as f64)
}

fn median(values: &[f64]) -> Result<f64, String> {
    if values.is_empty() {
        return Err("cannot calculate a median without observations".to_owned());
    }

    let mut values = values.to_vec();
    values.sort_by(f64::total_cmp);
    let middle = values.len() / 2;

    if values.len().is_multiple_of(2) {
        Ok((values[middle - 1] + values[middle]) / 2.0)
    } else {
        Ok(values[middle])
    }
}

fn bootstrap_median_ci(values: &[f64], seed: u64) -> Result<(f64, f64), String> {
    if values.is_empty() {
        return Err("cannot bootstrap without observations".to_owned());
    }

    let mut state = seed.max(1);
    let mut estimates = Vec::with_capacity(10_000);
    let mut sample = Vec::with_capacity(values.len());

    for _ in 0..10_000 {
        sample.clear();

        for _ in 0..values.len() {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            sample.push(values[(state as usize) % values.len()]);
        }

        estimates.push(median(&sample)?);
    }

    estimates.sort_by(f64::total_cmp);
    Ok((estimates[249], estimates[9_749]))
}

fn metric_seed(model: &str, mode: &str, metric: &str) -> u64 {
    let mut hasher = Sha256::new();
    hasher.update(model.as_bytes());
    hasher.update([0]);
    hasher.update(mode.as_bytes());
    hasher.update([0]);
    hasher.update(metric.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 8];
    bytes.copy_from_slice(&digest[..8]);

    u64::from_le_bytes(bytes)
}

fn session_break_even_turns(
    runs: &HashMap<RunKey, CampaignRun>,
    model: &str,
    mode: &str,
) -> Result<Vec<f64>, String> {
    let mut results = Vec::new();

    for repetition in 1..=RELEASE_REPETITIONS {
        for session_number in 1..=RELEASE_SESSION_COUNT {
            let session_id = format!("session-{session_number:03}");
            let baseline = get_run(runs, model, "italian", &session_id, repetition)?;
            let target = get_run(runs, model, mode, &session_id, repetition)?;

            if baseline.turns.len() != 10 || target.turns.len() != 10 {
                return Err(format!("{session_id} is not a ten-turn session"));
            }

            let mut baseline_total = 0_u64;
            let mut target_total = 0_u64;
            let mut break_even = 11_f64;

            for index in 0..10 {
                baseline_total +=
                    baseline.turns[index].input_tokens + baseline.turns[index].output_tokens;
                target_total +=
                    target.turns[index].input_tokens + target.turns[index].output_tokens;

                if target_total <= baseline_total && break_even == 11.0 {
                    break_even = (index + 1) as f64;
                }
            }

            results.push(break_even);
        }
    }

    Ok(results)
}

fn command_gate(release_dir: &Path) -> Result<bool, String> {
    let path = release_dir.join("compatibility/results.json");

    if !path.is_file() {
        return Ok(false);
    }

    let value = read_json(&path)?;
    let campaign = read_json(&release_dir.join("campaign.json"))?;
    let expected_codex_version = campaign
        .get("codex_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "campaign metadata lacks Codex version".to_owned())?;
    let checks = value
        .get("checks")
        .and_then(Value::as_array)
        .ok_or_else(|| format!("{} lacks checks[]", path.display()))?;
    let expected = [
        "chooser",
        "ammodino",
        "arranda",
        "status_de",
        "spengi",
        "inline_task",
        "switching",
        "new_session_reset",
        "resume",
        "compaction",
    ];

    if value.get("status").and_then(Value::as_str) != Some("complete")
        || checks.len() != expected.len() * MODELS.len()
    {
        return Ok(false);
    }

    let checks_pass = MODELS.iter().all(|model| {
        expected.iter().all(|name| {
            checks.iter().any(|check| {
                check.get("model").and_then(Value::as_str) == Some(model)
                    && check.get("name").and_then(Value::as_str) == Some(name)
                    && check.get("passed").and_then(Value::as_bool) == Some(true)
            })
        })
    });

    if !checks_pass {
        return Ok(false);
    }

    for model in MODELS {
        let transcript_path = release_dir
            .join("compatibility")
            .join(format!("{}-transcript.json", safe_component(model)));
        let judge_path = release_dir
            .join("compatibility")
            .join(format!("{}-judge.json", safe_component(model)));

        if !judge_path.is_file() {
            return Ok(false);
        }

        let transcript = read_json(&transcript_path)?;
        let judge = read_json(&judge_path)?;

        if transcript.get("model").and_then(Value::as_str) != Some(model)
            || transcript.get("codex_version").and_then(Value::as_str)
                != Some(expected_codex_version)
            || transcript
                .get("compaction_event_observed")
                .and_then(Value::as_bool)
                != Some(true)
            || judge.get("schema_version").and_then(Value::as_u64) != Some(1)
            || judge.get("benchmark_model").and_then(Value::as_str) != Some(model)
            || judge.get("judge_model").and_then(Value::as_str) != Some("gpt-5.6-sol")
            || judge.get("codex_version").and_then(Value::as_str) != Some(expected_codex_version)
            || judge
                .get("execution")
                .and_then(|execution| execution.get("visible_output"))
                .and_then(Value::as_str)
                .is_none_or(str::is_empty)
        {
            return Ok(false);
        }
    }

    Ok(true)
}

fn render_report_markdown(report: &Value) -> Result<String, String> {
    let release = report
        .get("release")
        .and_then(Value::as_str)
        .ok_or_else(|| "report has no release".to_owned())?;
    let ready = report
        .get("release_ready")
        .and_then(Value::as_bool)
        .ok_or_else(|| "report has no release readiness".to_owned())?;
    let rows = report
        .get("metrics")
        .and_then(Value::as_array)
        .ok_or_else(|| "report has no metrics".to_owned())?;
    let mut output = format!(
        "# Toen {release} Benchmark Report\n\nRelease ready: **{}**. The terse-Italian comparison is always shown, including losses.\n\n| Model | Mode | Median Reduction vs Italian | Reduction vs Terse Italian | Break-Even Turn | Correctness Difference | Style ≥3 | Protected | Passed |\n| --- | --- | ---: | ---: | ---: | ---: | ---: | ---: | --- |\n",
        if ready { "yes" } else { "no" }
    );

    for row in rows {
        output.push_str(&format!(
            "| {} | {} | {:.1}% | {:.1}% | {:.1} | {:.2} | {:.1}% | {:.1}% | {} |\n",
            row.get("model").and_then(Value::as_str).unwrap_or("?"),
            row.get("mode").and_then(Value::as_str).unwrap_or("?"),
            row.get("median_visible_output_reduction_vs_italian")
                .and_then(Value::as_f64)
                .unwrap_or_default()
                * 100.0,
            row.get("median_visible_output_reduction_vs_terse_italian")
                .and_then(Value::as_f64)
                .unwrap_or_default()
                * 100.0,
            row.get("median_break_even_turn")
                .and_then(Value::as_f64)
                .unwrap_or_default(),
            row.get("median_correctness_difference_vs_terse")
                .and_then(Value::as_f64)
                .unwrap_or_default(),
            row.get("style_score_at_least_3_rate")
                .and_then(Value::as_f64)
                .unwrap_or_default()
                * 100.0,
            row.get("protected_literal_preservation_rate")
                .and_then(Value::as_f64)
                .unwrap_or_default()
                * 100.0,
            if row.get("passed").and_then(Value::as_bool) == Some(true) {
                "yes"
            } else {
                "no"
            }
        ));
    }

    output.push_str(
        "\nProvider-reported usage, randomized blind inputs, judge outputs, fixture results, and campaign metadata are included in the release evidence archive.\n",
    );
    Ok(output)
}

fn validate_judge_manifest(release_dir: &Path, release: &str) -> Result<String, String> {
    let path = release_dir.join("judge/manifest.json");
    let manifest = read_json(&path)?;
    let expected_pairs = MODELS.len() * 2 * RELEASE_REPETITIONS * RELEASE_JUDGED_REPLIES;
    let codex_version = manifest
        .get("codex_version")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| format!("{} lacks a Codex version", path.display()))?;

    if manifest.get("schema_version").and_then(Value::as_u64) != Some(1)
        || manifest.get("release").and_then(Value::as_str) != Some(release)
        || manifest.get("status").and_then(Value::as_str) != Some("complete")
        || manifest.get("judge_model").and_then(Value::as_str) != Some("gpt-5.6-sol")
        || manifest.get("reasoning").and_then(Value::as_str) != Some("medium")
        || manifest.get("pairs").and_then(Value::as_u64) != Some(expected_pairs as u64)
        || manifest.get("rubric").and_then(Value::as_str) != Some("benchmarks/rubric.md")
        || manifest.get("schema").and_then(Value::as_str) != Some("benchmarks/judge.schema.json")
    {
        return Err(format!("{} is incomplete or inconsistent", path.display()));
    }

    Ok(codex_version.to_owned())
}

pub fn release_gates_pass(root: &Path, version: &str) -> Result<(), String> {
    let release_dir = root.join("benchmarks/releases").join(version);
    let report_path = release_dir.join("report.json");
    let value = read_json(&report_path)?;
    let campaign_file = read_json(&release_dir.join("campaign.json"))?;

    if value.get("release").and_then(Value::as_str) != Some(version)
        || value.get("release_ready").and_then(Value::as_bool) != Some(true)
        || value
            .get("comparison_with_terse_italian_disclosed")
            .and_then(Value::as_bool)
            != Some(true)
        || value
            .get("command_compatibility_passed")
            .and_then(Value::as_bool)
            != Some(true)
    {
        return Err(format!(
            "benchmark report for {version} has not passed every release gate"
        ));
    }

    let campaign = value
        .get("campaign")
        .and_then(Value::as_object)
        .ok_or_else(|| "benchmark report lacks campaign metadata".to_owned())?;

    if value.get("campaign") != Some(&campaign_file) {
        return Err("benchmark report campaign metadata differs from campaign.json".to_owned());
    }
    let models = campaign
        .get("models")
        .and_then(Value::as_array)
        .ok_or_else(|| "campaign metadata lacks models".to_owned())?;
    let campaign_codex_version = campaign
        .get("codex_version")
        .and_then(Value::as_str)
        .ok_or_else(|| "campaign metadata lacks Codex version".to_owned())?;

    if campaign.get("status").and_then(Value::as_str) != Some("complete")
        || campaign.get("release").and_then(Value::as_str) != Some(version)
        || campaign.get("repository_version").and_then(Value::as_str) != Some(version)
        || campaign.get("adapter").and_then(Value::as_str) != Some("codex")
        || campaign
            .get("codex_version")
            .and_then(Value::as_str)
            .is_none_or(str::is_empty)
        || campaign.get("reasoning").and_then(Value::as_str) != Some("medium")
        || campaign
            .get("visible_output_encoding")
            .and_then(Value::as_str)
            != Some("o200k_base")
        || campaign
            .get("single_turn_scenarios")
            .and_then(Value::as_u64)
            != Some(RELEASE_SCENARIO_COUNT as u64)
        || campaign.get("ten_turn_sessions").and_then(Value::as_u64)
            != Some(RELEASE_SESSION_COUNT as u64)
        || campaign.get("repetitions").and_then(Value::as_u64) != Some(RELEASE_REPETITIONS as u64)
        || campaign.get("completed_runs").and_then(Value::as_u64)
            != Some(
                (MODELS.len()
                    * CONDITIONS.len()
                    * RELEASE_REPETITIONS
                    * (RELEASE_SCENARIO_COUNT + RELEASE_SESSION_COUNT)) as u64,
            )
        || campaign.get("conditions").and_then(Value::as_array)
            != Some(
                &CONDITIONS
                    .iter()
                    .map(|condition| Value::String((*condition).to_owned()))
                    .collect::<Vec<_>>(),
            )
        || models
            != &MODELS
                .iter()
                .map(|model| Value::String((*model).to_owned()))
                .collect::<Vec<_>>()
    {
        return Err("benchmark campaign metadata is incomplete or inconsistent".to_owned());
    }

    let metrics = value
        .get("metrics")
        .and_then(Value::as_array)
        .ok_or_else(|| "benchmark report lacks metrics".to_owned())?;
    let expected_rows = MODELS.len() * 2;

    if metrics.len() != expected_rows
        || metrics.iter().any(|row| {
            row.get("samples").and_then(Value::as_u64)
                != Some((RELEASE_SCENARIO_COUNT * RELEASE_REPETITIONS) as u64)
                || row.get("judged_replies").and_then(Value::as_u64)
                    != Some((RELEASE_JUDGED_REPLIES * RELEASE_REPETITIONS) as u64)
                || !MODELS.contains(&row.get("model").and_then(Value::as_str).unwrap_or_default())
                || !["ammodino", "arranda"]
                    .contains(&row.get("mode").and_then(Value::as_str).unwrap_or_default())
                || row.get("passed").and_then(Value::as_bool) != Some(true)
                || row
                    .get("gates")
                    .and_then(Value::as_object)
                    .is_none_or(|gates| gates.values().any(|value| value != &Value::Bool(true)))
        })
    {
        return Err("benchmark metric rows have not all passed".to_owned());
    }

    let single_runs = load_runs(&release_dir.join("raw/single"))?;
    let session_runs = load_runs(&release_dir.join("raw/sessions"))?;
    let judgments: Vec<JudgedPair> = read_struct(&release_dir.join("judge/results.json"))?;
    let judge_codex_version = validate_judge_manifest(&release_dir, version)?;
    let report_markdown = fs::read_to_string(release_dir.join("report.md"))
        .map_err(|error| format!("read benchmark report Markdown: {error}"))?;
    let metric_pairs = metrics
        .iter()
        .filter_map(|row| {
            Some((
                row.get("model")?.as_str()?.to_owned(),
                row.get("mode")?.as_str()?.to_owned(),
            ))
        })
        .collect::<std::collections::HashSet<_>>();

    if single_runs.len()
        != MODELS.len() * CONDITIONS.len() * RELEASE_REPETITIONS * RELEASE_SCENARIO_COUNT
        || session_runs.len()
            != MODELS.len() * CONDITIONS.len() * RELEASE_REPETITIONS * RELEASE_SESSION_COUNT
        || judgments.len() != MODELS.len() * 2 * RELEASE_REPETITIONS * RELEASE_JUDGED_REPLIES
        || metric_pairs.len() != expected_rows
        || !complete_run_grid(&single_runs, version, campaign_codex_version, false)
        || !complete_run_grid(&session_runs, version, campaign_codex_version, true)
        || !complete_judgment_grid(&judgments, version, &judge_codex_version)
        || !command_gate(&release_dir)?
        || report_markdown != render_report_markdown(&value)?
    {
        return Err("benchmark evidence set is incomplete".to_owned());
    }

    Ok(())
}

fn complete_run_grid(
    runs: &[CampaignRun],
    version: &str,
    codex_version: &str,
    session: bool,
) -> bool {
    let expected_items = if session {
        RELEASE_SESSION_COUNT
    } else {
        RELEASE_SCENARIO_COUNT
    };
    let prefix = if session { "session" } else { "scenario" };
    let expected_turns = if session { 10 } else { 1 };
    let indexed = index_runs(runs);

    if indexed.len() != runs.len() {
        return false;
    }

    MODELS.iter().all(|model| {
        CONDITIONS.iter().all(|condition| {
            (1..=RELEASE_REPETITIONS).all(|repetition| {
                (1..=expected_items).all(|number| {
                    let id = format!("{prefix}-{number:03}");
                    let Some(run) = indexed.get(&(
                        (*model).to_owned(),
                        (*condition).to_owned(),
                        id.clone(),
                        repetition,
                    )) else {
                        return false;
                    };

                    run.release == version
                        && run.model == *model
                        && run.condition == *condition
                        && run.scenario_id == id
                        && run.session == session
                        && run.completed
                        && run.reasoning == "medium"
                        && run.codex_version == codex_version
                        && run.turns.len() == expected_turns
                        && run.turns.iter().enumerate().all(|(index, turn)| {
                            turn.turn == index + 1
                                && !turn.visible_output.trim().is_empty()
                                && turn.input_tokens > 0
                                && turn.output_tokens > 0
                                && count_visible_tokens(&turn.visible_output)
                                    == Ok(turn.visible_output_tokens)
                                && turn.protected_preserved
                                    == turn.protected.iter().all(|literal| {
                                        contains_literal(&turn.visible_output, literal)
                                    })
                        })
                })
            })
        })
    })
}

fn complete_judgment_grid(
    judgments: &[JudgedPair],
    release: &str,
    judge_codex_version: &str,
) -> bool {
    let indexed = judgments
        .iter()
        .map(|judgment| (judgment.id.as_str(), judgment))
        .collect::<HashMap<_, _>>();

    if indexed.len() != judgments.len()
        || judgments.len() != MODELS.len() * 2 * RELEASE_REPETITIONS * RELEASE_JUDGED_REPLIES
    {
        return false;
    }

    MODELS.iter().all(|model| {
        ["ammodino", "arranda"].iter().all(|mode| {
            (1..=RELEASE_REPETITIONS).all(|repetition| {
                let singles_complete = (1..=RELEASE_SCENARIO_COUNT).all(|number| {
                    let scenario_id = format!("scenario-{number:03}");
                    let id = format!(
                        "{}__{}__{}__r{repetition}",
                        safe_component(model),
                        mode,
                        scenario_id
                    );
                    let Some(judgment) = indexed.get(id.as_str()) else {
                        return false;
                    };
                    let expected_side = if deterministic_side(release, &id) {
                        "a"
                    } else {
                        "b"
                    };

                    valid_judgment(
                        judgment,
                        model,
                        mode,
                        &scenario_id,
                        repetition,
                        false,
                        1,
                        expected_side,
                        judge_codex_version,
                    )
                });
                let sessions_complete = (1..=RELEASE_SESSION_COUNT).all(|number| {
                    let session_id = format!("session-{number:03}");

                    (1..=10).all(|turn| {
                        let id = format!(
                            "{}__{}__{}__turn-{turn:02}__r{repetition}",
                            safe_component(model),
                            mode,
                            session_id
                        );
                        let Some(judgment) = indexed.get(id.as_str()) else {
                            return false;
                        };
                        let expected_side = if deterministic_side(release, &id) {
                            "a"
                        } else {
                            "b"
                        };

                        valid_judgment(
                            judgment,
                            model,
                            mode,
                            &session_id,
                            repetition,
                            true,
                            turn,
                            expected_side,
                            judge_codex_version,
                        )
                    })
                });

                singles_complete && sessions_complete
            })
        })
    })
}

#[allow(clippy::too_many_arguments)]
fn valid_judgment(
    judgment: &JudgedPair,
    model: &str,
    mode: &str,
    scenario_id: &str,
    repetition: usize,
    session: bool,
    turn: usize,
    expected_side: &str,
    judge_codex_version: &str,
) -> bool {
    judgment.benchmark_model == model
        && judgment.target_mode == mode
        && judgment.scenario_id == scenario_id
        && judgment.repetition == repetition
        && judgment.session == session
        && judgment.turn == turn
        && judgment.target_side == expected_side
        && (0.0..=4.0).contains(&judgment.correctness_target)
        && (0.0..=4.0).contains(&judgment.correctness_terse)
        && judgment.style_target <= 4
        && judgment.style_terse <= 4
        && judgment.judge_model == "gpt-5.6-sol"
        && judgment.judge_codex_version == judge_codex_version
        && judgment.judge_input_tokens > 0
        && judgment.judge_output_tokens > 0
        && !judgment.raw_judge_output.trim().is_empty()
}

fn valid_release_version(value: &str) -> bool {
    if value.is_empty()
        || value == "."
        || value == ".."
        || value.starts_with(['.', '-'])
        || value.ends_with(['.', '-'])
    {
        return false;
    }

    let mut parts = value.splitn(2, '-');
    let core = parts.next().unwrap_or_default();
    let prerelease = parts.next();
    let core_parts = core.split('.').collect::<Vec<_>>();

    if core_parts.len() != 3
        || core_parts
            .iter()
            .any(|part| part.is_empty() || !part.bytes().all(|byte| byte.is_ascii_digit()))
    {
        return false;
    }

    prerelease.is_none_or(|suffix| {
        !suffix.is_empty()
            && suffix.split('.').all(|part| {
                !part.is_empty()
                    && part
                        .bytes()
                        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
            })
    })
}

fn read_json(path: &Path) -> Result<Value, String> {
    read_struct(path)
}

fn read_struct<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<T, String> {
    let text =
        fs::read_to_string(path).map_err(|error| format!("read {}: {error}", path.display()))?;

    serde_json::from_str(&text).map_err(|error| format!("parse {}: {error}", path.display()))
}

fn write_json(path: &Path, value: &Value) -> Result<(), String> {
    write_struct(path, value)
}

fn write_struct<T: Serialize + ?Sized>(path: &Path, value: &T) -> Result<(), String> {
    let text = serde_json::to_string_pretty(value)
        .map_err(|error| format!("serialize {}: {error}", path.display()))?;

    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent)
            .map_err(|error| format!("create {}: {error}", parent.display()))?;
    }

    fs::write(path, format!("{text}\n"))
        .map_err(|error| format!("write {}: {error}", path.display()))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;
    use std::sync::atomic::{AtomicUsize, Ordering};

    use super::*;

    static TEST_DIRECTORY_SEQUENCE: AtomicUsize = AtomicUsize::new(1);

    struct TestDirectory {
        path: PathBuf,
    }

    impl TestDirectory {
        fn new(label: &str) -> Self {
            let sequence = TEST_DIRECTORY_SEQUENCE.fetch_add(1, Ordering::Relaxed);
            let path = std::env::temp_dir()
                .join(format!("toen-{label}-{}-{sequence}", std::process::id()));
            fs::create_dir_all(&path).unwrap();

            Self { path }
        }
    }

    impl Drop for TestDirectory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.path);
        }
    }

    #[derive(Default)]
    struct MockAdapter {
        next_session: AtomicUsize,
        modes: Mutex<HashMap<String, String>>,
    }

    impl MockAdapter {
        fn response(
            &self,
            prompt: &str,
            output_schema: Option<&Path>,
            persistent: bool,
            session_id: Option<&str>,
            compacted: bool,
        ) -> ExecResult {
            let schema = output_schema
                .and_then(Path::file_name)
                .and_then(|name| name.to_str());
            let returned_session = session_id.map(str::to_owned).or_else(|| {
                persistent.then(|| {
                    format!(
                        "test-session-{}",
                        self.next_session.fetch_add(1, Ordering::Relaxed)
                    )
                })
            });
            let final_output = match schema {
                Some("judge.schema.json") => serde_json::json!({
                    "correctness_a": 4,
                    "correctness_b": 4,
                    "style_a": 4,
                    "style_b": 4,
                    "safety_violation_a": false,
                    "safety_violation_b": false,
                    "notes": "deterministic test judge"
                })
                .to_string(),
                Some("compatibility.schema.json") => serde_json::json!({
                    "chooser": true,
                    "ammodino": true,
                    "arranda": true,
                    "status_de": true,
                    "spengi": true,
                    "inline_task": true,
                    "switching": true,
                    "new_session_reset": true,
                    "resume": true,
                    "compaction": true,
                    "notes": "deterministic compatibility judge"
                })
                .to_string(),
                _ => {
                    let command = prompt.lines().last().unwrap_or(prompt).trim();
                    let mut modes = self.modes.lock().unwrap();
                    let mode = returned_session
                        .as_deref()
                        .and_then(|id| modes.get(id))
                        .cloned()
                        .unwrap_or_else(|| "spento".to_owned());

                    if command.starts_with("$toen ammodino") {
                        if let Some(id) = &returned_session {
                            modes.insert(id.clone(), "ammodino".to_owned());
                        }

                        format!("ammodino {command}")
                    } else if command.starts_with("$toen arranda") {
                        if let Some(id) = &returned_session {
                            modes.insert(id.clone(), "arranda".to_owned());
                        }

                        format!("arranda {command}")
                    } else if command.starts_with("$toen spengi") {
                        if let Some(id) = &returned_session {
                            modes.insert(id.clone(), "spento".to_owned());
                        }

                        format!("spento {command}")
                    } else if command == "$toen de" {
                        mode
                    } else if command == "$toen" {
                        "Ammodino or Arranda?".to_owned()
                    } else {
                        command.to_owned()
                    }
                }
            };

            ExecResult {
                visible_output: final_output.clone(),
                final_output,
                input_tokens: 20,
                output_tokens: 5,
                session_id: returned_session,
                compacted,
                stderr: "mock adapter\n".to_owned(),
            }
        }
    }

    impl HarnessAdapter for MockAdapter {
        fn name(&self) -> &'static str {
            "codex"
        }

        fn version(&self) -> Result<String, String> {
            Ok("codex-test 1.0.0".to_owned())
        }

        fn invoke(&self, invocation: &Invocation<'_>) -> Result<ExecResult, String> {
            Ok(self.response(
                invocation.prompt,
                invocation.output_schema,
                invocation.persistent,
                None,
                false,
            ))
        }

        fn resume(
            &self,
            _model: &str,
            session_id: &str,
            prompt: &str,
            output_schema: Option<&Path>,
        ) -> Result<ExecResult, String> {
            Ok(self.response(prompt, output_schema, true, Some(session_id), false))
        }

        fn invoke_configured(
            &self,
            invocation: &Invocation<'_>,
            _extra_config: &[&str],
        ) -> Result<ExecResult, String> {
            Ok(self.response(
                invocation.prompt,
                invocation.output_schema,
                invocation.persistent,
                None,
                true,
            ))
        }

        fn resume_configured(
            &self,
            _model: &str,
            session_id: &str,
            prompt: &str,
            output_schema: Option<&Path>,
            _extra_config: &[&str],
        ) -> Result<ExecResult, String> {
            Ok(self.response(prompt, output_schema, true, Some(session_id), true))
        }

        fn parse_events(&self, stdout: &[u8], stderr: &[u8]) -> Result<ExecResult, String> {
            CodexAdapter {
                binary: "codex".to_owned(),
            }
            .parse_events(stdout, stderr)
        }
    }

    fn prepare_protocol_root(path: &Path) {
        let repository = crate::repo_root().unwrap();
        let benchmarks = path.join("benchmarks");
        let skill = path.join("plugins/toen/skills/toen");
        fs::create_dir_all(&benchmarks).unwrap();
        fs::create_dir_all(&skill).unwrap();
        fs::write(skill.join("SKILL.md"), "# Test Toen Skill\n").unwrap();

        for name in [
            "scenarios.json",
            "sessions.json",
            "judge.schema.json",
            "compatibility.schema.json",
            "rubric.md",
        ] {
            fs::copy(
                repository.join("benchmarks").join(name),
                benchmarks.join(name),
            )
            .unwrap();
        }

        copy_directory(
            &repository.join("benchmarks/fixtures"),
            &benchmarks.join("fixtures"),
        )
        .unwrap();
    }

    fn synthetic_turn(
        turn: usize,
        condition: &str,
        fixture_test_passed: Option<bool>,
    ) -> TurnResult {
        let repeated = match condition {
            "italian" => "italiano ".repeat(100),
            "terse_italian" => "conciso ".repeat(40),
            "ammodino" => "dé ".repeat(20),
            "arranda" => "dé ".repeat(15),
            _ => unreachable!(),
        };
        let visible_output = format!("{repeated}KEEP");
        let (input_tokens, output_tokens) = match condition {
            "italian" => (100, 100),
            "terse_italian" => (40, 40),
            "ammodino" | "arranda" => (10, 10),
            _ => unreachable!(),
        };

        TurnResult {
            turn,
            prompt: format!("Synthetic turn {turn}"),
            visible_output_tokens: count_visible_tokens(&visible_output).unwrap(),
            visible_output,
            input_tokens,
            output_tokens,
            session_id: Some(format!("synthetic-session-{turn}")),
            compacted: false,
            protected: vec!["KEEP".to_owned()],
            protected_preserved: true,
            fixture_test_passed,
            fixture_test_output: fixture_test_passed.map(|_| "fixture passed".to_owned()),
            stderr: String::new(),
        }
    }

    fn write_complete_campaign(path: &Path, release: &str) {
        let release_dir = path.join("benchmarks/releases").join(release);
        let single_dir = release_dir.join("raw/single");
        let session_dir = release_dir.join("raw/sessions");
        fs::create_dir_all(&single_dir).unwrap();
        fs::create_dir_all(&session_dir).unwrap();

        for model in MODELS {
            for condition in CONDITIONS {
                for repetition in 1..=RELEASE_REPETITIONS {
                    for number in 1..=RELEASE_SCENARIO_COUNT {
                        let scenario_id = format!("scenario-{number:03}");
                        let run = CampaignRun {
                            schema_version: 1,
                            release: release.to_owned(),
                            model: model.to_owned(),
                            condition: condition.to_owned(),
                            scenario_id: scenario_id.clone(),
                            language: "english".to_owned(),
                            kind: "testing".to_owned(),
                            repetition,
                            session: false,
                            completed: true,
                            codex_version: "codex-test 1.0.0".to_owned(),
                            reasoning: "medium".to_owned(),
                            turns: vec![synthetic_turn(1, condition, Some(true))],
                        };

                        write_struct(
                            &single_dir.join(run_filename(
                                model,
                                condition,
                                &scenario_id,
                                repetition,
                            )),
                            &run,
                        )
                        .unwrap();
                    }

                    for number in 1..=RELEASE_SESSION_COUNT {
                        let session_id = format!("session-{number:03}");
                        let run = CampaignRun {
                            schema_version: 1,
                            release: release.to_owned(),
                            model: model.to_owned(),
                            condition: condition.to_owned(),
                            scenario_id: session_id.clone(),
                            language: "english".to_owned(),
                            kind: "testing".to_owned(),
                            repetition,
                            session: true,
                            completed: true,
                            codex_version: "codex-test 1.0.0".to_owned(),
                            reasoning: "medium".to_owned(),
                            turns: (1..=10)
                                .map(|turn| synthetic_turn(turn, condition, None))
                                .collect(),
                        };

                        write_struct(
                            &session_dir.join(run_filename(
                                model,
                                condition,
                                &session_id,
                                repetition,
                            )),
                            &run,
                        )
                        .unwrap();
                    }
                }
            }
        }

        write_json(
            &release_dir.join("campaign.json"),
            &serde_json::json!({
                "schema_version": 1,
                "release": release,
                "repository_version": release,
                "status": "complete",
                "adapter": "codex",
                "codex_version": "codex-test 1.0.0",
                "models": MODELS,
                "reasoning": "medium",
                "conditions": CONDITIONS,
                "visible_output_encoding": "o200k_base",
                "single_turn_scenarios": RELEASE_SCENARIO_COUNT,
                "ten_turn_sessions": RELEASE_SESSION_COUNT,
                "repetitions": RELEASE_REPETITIONS,
                "completed_runs": MODELS.len()
                    * CONDITIONS.len()
                    * RELEASE_REPETITIONS
                    * (RELEASE_SCENARIO_COUNT + RELEASE_SESSION_COUNT),
                "user_config": "ignored",
                "user_rules": "ignored"
            }),
        )
        .unwrap();
    }

    #[test]
    fn parses_real_codex_jsonl_shape() {
        let adapter = CodexAdapter {
            binary: "codex".to_owned(),
        };
        let stdout = r#"{"type":"thread.started","thread_id":"abc"}
{"type":"item.completed","item":{"type":"agent_message","text":"Dé, fatto."}}
{"type":"turn.completed","usage":{"input_tokens":31,"output_tokens":7}}
"#;
        let result = adapter.parse_events(stdout.as_bytes(), b"").unwrap();

        assert_eq!(result.visible_output, "Dé, fatto.");
        assert_eq!(result.final_output, "Dé, fatto.");
        assert_eq!(result.input_tokens, 31);
        assert_eq!(result.output_tokens, 7);
        assert_eq!(result.session_id.as_deref(), Some("abc"));
        assert!(!result.compacted);
    }

    #[test]
    fn joins_every_visible_agent_message() {
        let adapter = CodexAdapter {
            binary: "codex".to_owned(),
        };
        let stdout = r#"{"type":"thread.started","thread_id":"abc"}
{"type":"item.completed","item":{"type":"agent_message","text":"Checking now."}}
{"type":"item.completed","item":{"type":"agent_message","text":"Dé, fatto."}}
{"type":"turn.completed","usage":{"input_tokens":31,"output_tokens":7}}
"#;
        let result = adapter.parse_events(stdout.as_bytes(), b"").unwrap();

        assert_eq!(result.visible_output, "Checking now.\n\nDé, fatto.");
        assert_eq!(result.final_output, "Dé, fatto.");
        assert!(count_visible_tokens(&result.visible_output).unwrap() > 0);
    }

    #[test]
    fn shared_codex_arguments_are_resume_safe() {
        let adapter = CodexAdapter {
            binary: "codex".to_owned(),
        };
        let arguments = adapter.common_args("gpt-5.6-luna");

        assert!(!arguments.iter().any(|argument| argument == "--color"));
        assert!(arguments.iter().any(|argument| argument == "--json"));
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "--ignore-user-config")
        );
        assert!(
            arguments
                .iter()
                .any(|argument| argument == "--ignore-rules")
        );
    }

    #[test]
    fn records_actual_compaction_events() {
        let adapter = CodexAdapter {
            binary: "codex".to_owned(),
        };
        let stdout = r#"{"type":"thread.started","thread_id":"abc"}
{"type":"context.compacted"}
{"type":"item.completed","item":{"type":"agent_message","text":"arranda"}}
{"type":"turn.completed","usage":{"input_tokens":80,"output_tokens":2}}
"#;
        let result = adapter.parse_events(stdout.as_bytes(), b"").unwrap();

        assert!(result.compacted);
    }

    #[test]
    fn statistics_are_deterministic_and_handle_even_samples() {
        assert_eq!(median(&[4.0, 1.0, 3.0, 2.0]).unwrap(), 2.5);
        assert_eq!(median(&[9.0, 3.0, 6.0]).unwrap(), 6.0);
        assert!(median(&[]).is_err());

        let first = bootstrap_median_ci(&[0.1, 0.2, 0.3, 0.4], 42).unwrap();
        let second = bootstrap_median_ci(&[0.1, 0.2, 0.3, 0.4], 42).unwrap();

        assert_eq!(first, second);
        assert!(first.0 <= 0.25 && first.1 >= 0.25);
    }

    #[test]
    fn benchmark_workers_are_bounded_and_batches_propagate_failures() {
        assert_eq!(parse_benchmark_workers(None).unwrap(), 1);
        assert_eq!(parse_benchmark_workers(Some("8".into())).unwrap(), 8);
        assert!(parse_benchmark_workers(Some("0".into())).is_err());
        assert!(parse_benchmark_workers(Some("17".into())).is_err());
        assert!(parse_benchmark_workers(Some("many".into())).is_err());

        let completed = Mutex::new(Vec::new());
        run_in_batches(&[1, 2, 3, 4], 2, &|item| {
            completed.lock().unwrap().push(*item);
            Ok(())
        })
        .unwrap();
        let mut completed = completed.into_inner().unwrap();
        completed.sort_unstable();

        assert_eq!(completed, [1, 2, 3, 4]);
        assert!(
            run_in_batches(&[1, 2, 3], 2, &|item| {
                if *item == 2 {
                    Err("expected worker failure".to_owned())
                } else {
                    Ok(())
                }
            })
            .is_err()
        );
    }

    #[test]
    fn release_versions_are_path_safe() {
        assert!(valid_release_version("0.1.0"));
        assert!(valid_release_version("0.1.0-rc.1"));
        assert!(!valid_release_version("../escape"));
        assert!(!valid_release_version("0.1"));
    }

    #[test]
    fn condition_prompts_keep_protected_tasks_exact() {
        let task = "Preserve `cargo test --locked` and /tmp/a.txt.";
        let prompt = condition_prompt("arranda", task, "# Skill").unwrap();

        assert!(prompt.contains(task));
        assert!(prompt.contains("$toen arranda"));
    }

    #[test]
    fn protected_word_literals_require_boundaries() {
        assert!(contains_literal("Il comando `de` resta ASCII.", "de"));
        assert!(contains_literal("Scrivi dé.", "dé"));
        assert!(!contains_literal("La modalità dipende dal comando.", "de"));
        assert!(contains_literal(
            "Run `cargo test --locked`.",
            "cargo test --locked"
        ));
    }

    #[test]
    fn benchmark_argument_and_prompt_helpers_reject_bad_inputs() {
        assert_eq!(
            release_args(&["--release".to_owned(), "0.1.0".to_owned()], false).unwrap(),
            ("0.1.0".to_owned(), false)
        );
        assert_eq!(
            release_args(
                &[
                    "--release".to_owned(),
                    "0.1.0-rc.1".to_owned(),
                    "--resume".to_owned(),
                ],
                true,
            )
            .unwrap(),
            ("0.1.0-rc.1".to_owned(), true)
        );

        assert!(release_args(&["0.1.0".to_owned()], false).is_err());
        assert!(release_args(&["--release".to_owned(), "../escape".to_owned()], false).is_err());
        assert!(condition_prompt("unknown", "task", "skill").is_err());
    }

    #[test]
    fn report_markdown_discloses_terse_comparison() {
        let report = serde_json::json!({
            "release": "0.1.0",
            "release_ready": false,
            "metrics": [{
                "model": "gpt-5.6-sol",
                "mode": "ammodino",
                "median_visible_output_reduction_vs_italian": 0.2,
                "median_visible_output_reduction_vs_terse_italian": -0.1,
                "median_break_even_turn": 8.0,
                "median_correctness_difference_vs_terse": -0.05,
                "style_score_at_least_3_rate": 0.95,
                "protected_literal_preservation_rate": 1.0,
                "passed": false
            }]
        });
        let markdown = render_report_markdown(&report).unwrap();

        assert!(markdown.contains("Release ready: **no**"));
        assert!(markdown.contains("The terse-Italian comparison is always shown"));
        assert!(markdown.contains("| gpt-5.6-sol | ammodino | 20.0% | -10.0%"));
    }

    #[test]
    fn judge_score_validation_enforces_the_rubric_range() {
        let valid = JudgeScores {
            correctness_a: 4,
            correctness_b: 0,
            style_a: 3,
            style_b: 2,
            safety_violation_a: false,
            safety_violation_b: false,
            notes: "within range".to_owned(),
        };
        let invalid = JudgeScores {
            correctness_a: 5,
            correctness_b: 0,
            style_a: 3,
            style_b: 2,
            safety_violation_a: false,
            safety_violation_b: false,
            notes: "outside range".to_owned(),
        };

        assert!(validate_judge_scores(&valid, "valid").is_ok());
        assert!(validate_judge_scores(&invalid, "invalid").is_err());
    }

    #[test]
    fn resumed_runs_and_judgments_require_matching_runtime_identity() {
        let run = CampaignRun {
            schema_version: 1,
            release: "0.1.0".to_owned(),
            model: "gpt-5.6-sol".to_owned(),
            condition: "ammodino".to_owned(),
            scenario_id: "session-001".to_owned(),
            language: "english".to_owned(),
            kind: "diagnosis".to_owned(),
            repetition: 1,
            session: true,
            completed: false,
            codex_version: "codex-cli 1".to_owned(),
            reasoning: "medium".to_owned(),
            turns: vec![TurnResult {
                turn: 1,
                prompt: "Diagnose it.".to_owned(),
                visible_output: "Check Cargo.lock.".to_owned(),
                visible_output_tokens: 4,
                input_tokens: 20,
                output_tokens: 4,
                session_id: Some("thread-1".to_owned()),
                compacted: false,
                protected: Vec::new(),
                protected_preserved: true,
                fixture_test_passed: None,
                fixture_test_output: None,
                stderr: String::new(),
            }],
        };
        let expected = RunExpectation {
            release: "0.1.0",
            model: "gpt-5.6-sol",
            condition: "ammodino",
            scenario_id: "session-001",
            language: "english",
            kind: "diagnosis",
            repetition: 1,
            session: true,
            codex_version: "codex-cli 1",
            expected_turns: 10,
        };

        assert!(validate_saved_run(&run, &expected, Path::new("run.json")).is_ok());

        let stale_expected = RunExpectation {
            codex_version: "codex-cli 2",
            ..expected
        };

        assert!(validate_saved_run(&run, &stale_expected, Path::new("run.json")).is_err());

        let judgment = JudgedPair {
            id: "id".to_owned(),
            benchmark_model: "gpt-5.6-sol".to_owned(),
            judge_model: "gpt-5.6-sol".to_owned(),
            judge_codex_version: "codex-cli 1".to_owned(),
            target_mode: "ammodino".to_owned(),
            scenario_id: "scenario-001".to_owned(),
            repetition: 1,
            session: false,
            turn: 1,
            target_side: "a".to_owned(),
            correctness_target: 4.0,
            correctness_terse: 4.0,
            style_target: 4,
            style_terse: 2,
            safety_violation_target: false,
            safety_violation_terse: false,
            judge_input_tokens: 20,
            judge_output_tokens: 5,
            raw_judge_output: "{\"correctness_a\":4}".to_owned(),
            judge_stderr: String::new(),
            notes: String::new(),
        };

        assert!(valid_judgment(
            &judgment,
            "gpt-5.6-sol",
            "ammodino",
            "scenario-001",
            1,
            false,
            1,
            "a",
            "codex-cli 1"
        ));
        assert!(!valid_judgment(
            &judgment,
            "gpt-5.6-sol",
            "ammodino",
            "scenario-001",
            1,
            false,
            1,
            "a",
            "codex-cli 2"
        ));
    }

    #[test]
    fn mock_adapter_runs_campaign_resume_and_compatibility_end_to_end() {
        let directory = TestDirectory::new("campaign");
        prepare_protocol_root(&directory.path);
        let fixture = directory.path.join("benchmarks/fixtures/simple");
        fs::create_dir_all(&fixture).unwrap();
        fs::write(
            fixture.join("fixture.json"),
            r#"{"test_command":["rustc","--version"]}"#,
        )
        .unwrap();
        fs::write(fixture.join("input.txt"), "fixture\n").unwrap();
        let scenarios = vec![
            Scenario {
                id: "scenario-001".to_owned(),
                language: "english".to_owned(),
                kind: "explanation".to_owned(),
                prompt: "Explain KEEP.".to_owned(),
                protected: vec!["KEEP".to_owned()],
                fixture: None,
            },
            Scenario {
                id: "scenario-002".to_owned(),
                language: "italian".to_owned(),
                kind: "implementation".to_owned(),
                prompt: "Preserva KEEP.".to_owned(),
                protected: vec!["KEEP".to_owned()],
                fixture: Some("simple".to_owned()),
            },
        ];
        let sessions = vec![SessionScenario {
            id: "session-001".to_owned(),
            language: "livornese".to_owned(),
            kind: "planning".to_owned(),
            turns: (1..=10)
                .map(|turn| SessionTurn {
                    prompt: format!("Turn {turn} KEEP"),
                    protected: vec!["KEEP".to_owned()],
                })
                .collect(),
        }];
        let adapter = MockAdapter::default();
        let models = ["gpt-5.6-luna"];

        execute_campaign_with_adapter(
            CampaignSpec {
                root: &directory.path,
                release: "0.1.0",
                repository_version: "0.1.0",
                scenarios: &scenarios,
                sessions: &sessions,
                models: &models,
                repetitions: 1,
                resume: false,
            },
            &adapter,
        )
        .unwrap();

        let release_dir = directory.path.join("benchmarks/releases/0.1.0");
        assert_eq!(load_runs(&release_dir.join("raw/single")).unwrap().len(), 8);
        assert_eq!(
            load_runs(&release_dir.join("raw/sessions")).unwrap().len(),
            4
        );

        execute_campaign_with_adapter(
            CampaignSpec {
                root: &directory.path,
                release: "0.1.0",
                repository_version: "0.1.0",
                scenarios: &scenarios,
                sessions: &sessions,
                models: &models,
                repetitions: 1,
                resume: true,
            },
            &adapter,
        )
        .unwrap();

        run_compatibility(
            &directory.path,
            &release_dir,
            "# Test Skill",
            &MODELS,
            "codex-test 1.0.0",
            &adapter,
        )
        .unwrap();
        assert!(command_gate(&release_dir).unwrap());

        run_compatibility(
            &directory.path,
            &release_dir,
            "# Test Skill",
            &MODELS,
            "codex-test 1.0.0",
            &adapter,
        )
        .unwrap();
    }

    #[test]
    fn complete_evidence_is_judged_reported_and_release_gated() {
        let directory = TestDirectory::new("release-evidence");
        prepare_protocol_root(&directory.path);
        write_complete_campaign(&directory.path, "0.1.0");
        let release_dir = directory.path.join("benchmarks/releases/0.1.0");
        let adapter = MockAdapter::default();

        run_compatibility(
            &directory.path,
            &release_dir,
            "# Test Skill",
            &MODELS,
            "codex-test 1.0.0",
            &adapter,
        )
        .unwrap();
        judge_release_with_adapter(
            &directory.path,
            &["--release".to_owned(), "0.1.0".to_owned()],
            &adapter,
        )
        .unwrap();
        let first_judge_inputs = fs::read(release_dir.join("judge/inputs.json")).unwrap();

        judge_release_with_adapter(
            &directory.path,
            &["--release".to_owned(), "0.1.0".to_owned()],
            &adapter,
        )
        .unwrap();
        let resumed_judge_inputs = fs::read(release_dir.join("judge/inputs.json")).unwrap();

        assert_eq!(first_judge_inputs, resumed_judge_inputs);

        report_release(
            &directory.path,
            &["--release".to_owned(), "0.1.0".to_owned()],
        )
        .unwrap();

        release_gates_pass(&directory.path, "0.1.0").unwrap();
        let report = read_json(&release_dir.join("report.json")).unwrap();

        assert_eq!(
            report.get("release_ready").and_then(Value::as_bool),
            Some(true)
        );
        assert_eq!(
            read_struct::<Vec<JudgedPair>>(&release_dir.join("judge/results.json"))
                .unwrap()
                .len(),
            MODELS.len() * 2 * RELEASE_REPETITIONS * RELEASE_JUDGED_REPLIES
        );
    }

    #[cfg(unix)]
    #[test]
    fn codex_adapter_executes_streams_resumes_and_reports_failures() {
        use std::os::unix::fs::PermissionsExt;

        let directory = TestDirectory::new("codex-process");
        let script = directory.path.join("fake-codex");
        fs::write(
            &script,
            r#"#!/bin/sh
if [ "$1" = "--version" ]; then
  echo "codex-test 1.0.0"
  exit 0
fi
saw_sandbox=0
saw_approval=0
for argument in "$@"; do
  if [ "$argument" = "FAIL" ]; then
    echo "forced failure" >&2
    exit 7
  fi
  if [ "$argument" = "--sandbox" ]; then
    saw_sandbox=1
  fi
  if [ "$argument" = "--approve-for-me" ]; then
    saw_approval=1
  fi
done
if [ "$saw_sandbox" = "1" ] && [ "$saw_approval" = "1" ]; then
  echo "sandbox and automatic approval conflict" >&2
  exit 9
fi
echo '{"type":"thread.started","thread_id":"process-session"}'
echo '{"type":"item.completed","item":{"type":"agent_message","text":"process output"}}'
echo '{"type":"turn.completed","usage":{"input_tokens":20,"output_tokens":5}}'
echo 'process stderr' >&2
"#,
        )
        .unwrap();
        fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
        let adapter = CodexAdapter {
            binary: script.display().to_string(),
        };
        let schema = directory.path.join("schema.json");
        let invocation = Invocation {
            model: "gpt-5.6-luna",
            prompt: "PASS",
            working_dir: &directory.path,
            writable: true,
            persistent: true,
            output_schema: Some(&schema),
        };

        assert_eq!(adapter.name(), "codex");
        assert_eq!(adapter.version().unwrap(), "codex-test 1.0.0");

        let invoked = adapter
            .invoke_configured(&invocation, &["model_context_window=32768"])
            .unwrap();
        assert_eq!(invoked.visible_output, "process output");
        assert_eq!(invoked.session_id.as_deref(), Some("process-session"));
        assert!(invoked.stderr.contains("process stderr"));

        let resumed = adapter
            .resume_configured(
                "gpt-5.6-luna",
                "process-session",
                "PASS",
                None,
                &["model_context_window=32768"],
            )
            .unwrap();
        assert_eq!(resumed.output_tokens, 5);

        let failure = Invocation {
            prompt: "FAIL",
            writable: false,
            persistent: false,
            output_schema: None,
            ..invocation
        };
        assert!(
            adapter
                .invoke(&failure)
                .unwrap_err()
                .contains("exit status: 7")
        );
        assert!(
            adapter
                .resume("gpt-5.6-luna", "process-session", "FAIL", None)
                .unwrap_err()
                .contains("exit status: 7")
        );
    }
}
