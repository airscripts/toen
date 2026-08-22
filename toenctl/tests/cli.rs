use std::process::Command;

fn toenctl() -> Command {
    let mut command = Command::new(env!("CARGO_BIN_EXE_toenctl"));
    command.current_dir(env!("CARGO_MANIFEST_DIR"));
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
fn smoke_benchmark_validates_balanced_scenarios() {
    let output = toenctl()
        .args(["bench", "smoke", "--check"])
        .output()
        .expect("run benchmark smoke check");

    assert!(output.status.success());
    assert!(String::from_utf8_lossy(&output.stdout).contains("non-spending CI manifest"));
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
fn release_packaging_refuses_missing_benchmark_evidence() {
    let output = toenctl()
        .args(["package", "--version", "0.1.0"])
        .output()
        .expect("run gated package command");

    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("report.json"));
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
