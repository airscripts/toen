use std::path::Path;
use std::process::Command;

fn toenctl() -> Command {
    toenctl_in(Path::new(env!("CARGO_MANIFEST_DIR")))
}

fn toenctl_in(directory: &Path) -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_toenctl"));
    command.current_dir(directory);
    command
}

#[test]
fn version_reports_the_workspace_version() {
    let output = toenctl().arg("version").output().expect("run toenctl");

    assert!(output.status.success());
    assert_eq!(
        String::from_utf8_lossy(&output.stdout).trim(),
        "toenctl 0.1.0"
    );
}

#[test]
fn corpus_check_covers_the_real_repository() {
    let output = toenctl()
        .args(["corpus", "check"])
        .output()
        .expect("run corpus check");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("500 accepted records"));
}

#[test]
fn metadata_source_check_does_not_need_network() {
    let output = toenctl()
        .args(["sources", "verify", "--metadata-only"])
        .output()
        .expect("run source check");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("metadata verification"));
}

#[test]
fn generated_assets_are_current() {
    let output = toenctl()
        .args(["generate", "--check"])
        .output()
        .expect("run generated-file check");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("up to date"));
}

#[test]
fn plugin_manifests_are_consistent() {
    let output = toenctl()
        .args(["manifests", "check"])
        .output()
        .expect("run manifest check");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("passed validation"));
}

#[test]
fn toenizer_reports_exact_input_metrics() {
    let output = toenctl()
        .args(["toenizer", "count", "--text", "città", "--format", "json"])
        .output()
        .expect("run toenizer");

    assert!(output.status.success());
    let json = String::from_utf8_lossy(&output.stdout);
    assert!(json.contains("\"schema_version\": 1"));
    assert!(json.contains("\"tokenizer\": \"o200k-base\""));
    assert!(json.contains("\"utf8_bytes\": 6"));
}

#[test]
fn nested_help_is_successful_stdout() {
    let output = toenctl()
        .args(["toenizer", "--help"])
        .output()
        .expect("run toenizer help");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: toenizer"));
    assert!(String::from_utf8_lossy(&output.stderr).is_empty());
}

#[test]
fn nested_help_is_available_outside_a_workspace() {
    let directory =
        std::env::temp_dir().join(format!("toen-cli-toenizer-help-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();

    let output = toenctl_in(&directory)
        .args(["toenizer", "--help"])
        .output()
        .expect("run toenizer help outside workspace");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("Usage: toenizer"));
    assert!(String::from_utf8_lossy(&output.stderr).is_empty());
    std::fs::remove_dir_all(directory).unwrap();
}

#[test]
fn mutating_commands_reject_unexpected_arguments() {
    for args in [
        &["generate", "--bogus"][..],
        &["package"][..],
        &["corpus", "check", "--bogus"][..],
    ] {
        let output = toenctl().args(args).output().expect("run invalid command");

        assert!(!output.status.success(), "accepted {args:?}");
    }
}

#[test]
fn smoke_manifest_is_current_without_spending_tokens() {
    let output = toenctl()
        .args(["bench", "smoke", "--check"])
        .output()
        .expect("run benchmark smoke check");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("non-spending CI manifest"));
}

#[test]
fn unknown_commands_fail_without_running_a_mutation() {
    let output = toenctl()
        .arg("not-a-command")
        .output()
        .expect("run toenctl");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("unknown command"));
}

#[test]
fn unknown_commands_are_invalid_outside_a_workspace() {
    let directory =
        std::env::temp_dir().join(format!("toen-cli-non-workspace-{}", std::process::id()));
    std::fs::create_dir_all(&directory).unwrap();

    let output = toenctl_in(&directory)
        .arg("not-a-command")
        .output()
        .expect("run toenctl outside workspace");

    assert!(!output.status.success());
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(stderr.contains("invalid arguments"));
    assert!(!stderr.contains("workspace error"));
    std::fs::remove_dir_all(directory).unwrap();
}
