//! Command-surface documentation for the thin `toenctl` binary.
//!
//! Subcommands are kept in the library so the binary and integration tests use
//! the same execution path.
use std::path::PathBuf;

use crate::ToenError;

pub(crate) fn without_workspace(
    args: Vec<String>,
) -> Result<(Option<PathBuf>, Vec<String>), ToenError> {
    if args.first().map(String::as_str) != Some("--workspace") {
        return Ok((None, args));
    }
    if args.len() < 3 {
        return Err(ToenError::InvalidArguments(
            "--workspace requires a path and command".to_owned(),
        ));
    }
    let workspace = PathBuf::from(&args[1]);
    Ok((Some(workspace), args[2..].to_vec()))
}

pub(crate) fn usage(version: &str) {
    println!(
        "toenctl {version}\n\nCommands:\n  corpus check\n  sources verify [--metadata-only]\n  manifests check\n  generate [--check]\n  bench smoke|run|judge|report\n  toenizer count|compare|report\n  verify\n  test\n  doctor\n  package --version <version>"
    );
}

pub(crate) fn run(args: Vec<String>) -> Result<(), ToenError> {
    let (explicit_workspace, args) = without_workspace(args)?;
    let command = args.first().map(String::as_str).unwrap_or("help");
    validate_command(&args)?;
    let root = if matches!(command, "help" | "--help" | "-h" | "version")
        || (command == "toenizer" && crate::toenizer::is_display_request(&args[1..]))
    {
        None
    } else {
        Some(
            crate::workspace::Workspace::discover(explicit_workspace.as_deref())?
                .root()
                .to_path_buf(),
        )
    };

    match command {
        "corpus" if args.len() == 2 && args[1] == "check" => {
            operation(crate::corpus_check(root.as_deref().unwrap()))
        }
        "sources" if args.len() == 2 && args[1] == "verify" => {
            operation(crate::sources::verify(root.as_deref().unwrap(), None))
        }
        "sources" if args.len() == 3 && args[1] == "verify" && args[2] == "--metadata-only" => {
            operation(crate::sources::verify(
                root.as_deref().unwrap(),
                Some("--metadata-only"),
            ))
        }
        "manifests" if args.len() == 2 && args[1] == "check" => {
            operation(crate::manifests::check(root.as_deref().unwrap()))
        }
        "generate" if args.len() == 1 => {
            operation(crate::generate(root.as_deref().unwrap(), false))
        }
        "generate" if args.len() == 2 && args[1] == "--check" => {
            operation(crate::generate(root.as_deref().unwrap(), true))
        }
        "bench" => operation(crate::bench::run(
            root.as_deref().unwrap(),
            &args[1..],
            crate::VERSION,
        )),
        "toenizer" => operation(crate::toenizer::run(&args[1..], root.as_deref())),
        "verify" if args.len() == 1 => operation(crate::verify(root.as_deref().unwrap())),
        "test" if args.len() == 1 => operation(crate::test(root.as_deref().unwrap())),
        "doctor" if args.len() == 1 => operation(crate::doctor(root.as_deref().unwrap())),
        "package" => operation(crate::package(root.as_deref().unwrap(), &args[1..])),
        "version" if args.len() == 1 => {
            println!("toenctl {}", crate::VERSION);
            Ok(())
        }
        "help" | "--help" | "-h" if args.len() == 1 => {
            usage(crate::VERSION);
            Ok(())
        }
        "corpus" | "sources" | "manifests" | "generate" | "version" => {
            Err(ToenError::InvalidArguments(format!(
                "invalid arguments for `{command}`; try `toenctl help`"
            )))
        }
        _ => Err(ToenError::InvalidArguments(format!(
            "unknown command `{command}`; try `toenctl help`"
        ))),
    }
}

fn operation(result: Result<(), String>) -> Result<(), ToenError> {
    result.map_err(ToenError::Operation)
}

fn validate_command(args: &[String]) -> Result<(), ToenError> {
    let command = args.first().map(String::as_str).unwrap_or("help");
    let valid = match command {
        "help" | "--help" | "-h" | "version" => args.len() == 1,
        "corpus" => args == ["corpus", "check"],
        "sources" => {
            args == ["sources", "verify"] || args == ["sources", "verify", "--metadata-only"]
        }
        "manifests" => args == ["manifests", "check"],
        "generate" => args.len() == 1 || args == ["generate", "--check"],
        "bench" => {
            crate::bench::validate_args(&args[1..]).map_err(ToenError::InvalidArguments)?;
            true
        }
        "toenizer" => {
            crate::toenizer::validate_args(&args[1..]).map_err(ToenError::InvalidArguments)?;
            true
        }
        "verify" | "test" | "doctor" => args.len() == 1,
        "package" => args.len() == 3 && args[1] == "--version",
        _ => {
            return Err(ToenError::InvalidArguments(format!(
                "unknown command `{command}`; try `toenctl help`"
            )));
        }
    };

    if valid {
        Ok(())
    } else {
        Err(ToenError::InvalidArguments(format!(
            "invalid arguments for `{command}`; try `toenctl help`"
        )))
    }
}
