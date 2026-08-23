//! Reproducible archive support is implemented by the maintainer package command.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use sha2::{Digest, Sha256};
use zip::ZipWriter;
use zip::write::FileOptions;

pub(crate) fn replace_owned_outputs(
    staging: &Path,
    dist: &Path,
    outputs: &[String],
) -> Result<(), String> {
    let backup = dist.join(format!(".toen-backup-{}", std::process::id()));
    if backup.exists() {
        return Err(format!(
            "package backup path already exists: {}",
            backup.display()
        ));
    }

    for output in outputs {
        let staged = staging.join(output);
        let destination = dist.join(output);
        if !staged.is_file() {
            return Err(format!("package staging is missing {}", staged.display()));
        }
        if destination.exists() && destination.is_dir() {
            return Err(format!(
                "owned package output is a directory: {}",
                destination.display()
            ));
        }
    }

    let mut backed_up = Vec::new();
    let mut installed = Vec::new();
    if let Err(error) = fs::create_dir(&backup) {
        return Err(format!("create package backup directory: {error}"));
    }

    let result = (|| {
        for output in outputs {
            let destination = dist.join(output);
            if destination.exists() {
                let saved = backup.join(output);
                fs::rename(&destination, &saved).map_err(|error| {
                    format!(
                        "backup owned package output {}: {error}",
                        destination.display()
                    )
                })?;
                backed_up.push((saved, destination));
            }
        }

        for output in outputs {
            let staged = staging.join(output);
            let destination = dist.join(output);
            fs::rename(&staged, &destination).map_err(|error| {
                format!("install package output {}: {error}", destination.display())
            })?;
            installed.push(destination);
        }
        Ok::<(), String>(())
    })();

    match result {
        Ok(()) => fs::remove_dir_all(&backup)
            .map_err(|error| format!("remove package backup directory: {error}")),
        Err(error) => {
            let rollback = rollback(&installed, &backed_up, &backup);
            match rollback {
                Ok(()) => Err(error),
                Err(rollback_error) => Err(format!("{error}; rollback failed: {rollback_error}")),
            }
        }
    }
}

fn rollback(
    installed: &[PathBuf],
    backed_up: &[(PathBuf, PathBuf)],
    backup: &Path,
) -> Result<(), String> {
    for destination in installed.iter().rev() {
        if destination.exists() {
            fs::remove_file(destination)
                .map_err(|error| format!("remove partially installed output: {error}"))?;
        }
    }
    for (saved, destination) in backed_up.iter().rev() {
        fs::rename(saved, destination)
            .map_err(|error| format!("restore owned package output: {error}"))?;
    }
    fs::remove_dir(backup).map_err(|error| format!("remove package backup directory: {error}"))
}

pub(crate) fn write_benchmark_zip(
    destination: &Path,
    root: &Path,
    version: &str,
) -> Result<(), String> {
    let temporary = destination.with_extension(format!("zip.tmp-{}", std::process::id()));
    let file = fs::File::create(&temporary)
        .map_err(|error| format!("create {}: {error}", temporary.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let release = root.join("benchmarks/releases").join(version);

    let result = add_directory_to_zip_filtered(&mut zip, options, &release, "release", &["work"])
        .and_then(|()| {
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
                .map_err(|error| format!("finish {}: {error}", temporary.display()))
                .map(|_| ())
        });

    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }
    replace_file(&temporary, destination)
}

fn add_directory_to_zip_filtered(
    zip: &mut ZipWriter<fs::File>,
    options: FileOptions,
    base: &Path,
    archive_prefix: &str,
    skipped_names: &[&str],
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
        if skipped_names.contains(&name.as_ref()) {
            continue;
        }
        let archive_path = format!("{archive_prefix}/{name}");
        let metadata = fs::symlink_metadata(&path)
            .map_err(|error| format!("inspect package path {}: {error}", path.display()))?;
        if metadata.file_type().is_symlink() {
            return Err(format!(
                "package input must not contain symlinks: {}",
                path.display()
            ));
        }
        if metadata.is_dir() {
            add_directory_to_zip_filtered(zip, options, &path, &archive_path, skipped_names)?;
        } else {
            add_file_to_zip(zip, options, &archive_path, &path)?;
        }
    }
    Ok(())
}

pub(crate) fn write_zip(destination: &Path, root: &Path, relative_dir: &str) -> Result<(), String> {
    write_zip_with_prefix(destination, root, relative_dir, "")
}

pub(crate) fn write_zip_with_prefix(
    destination: &Path,
    root: &Path,
    relative_dir: &str,
    archive_prefix: &str,
) -> Result<(), String> {
    let temporary = destination.with_extension(format!("zip.tmp-{}", std::process::id()));
    let file = fs::File::create(&temporary)
        .map_err(|error| format!("create {}: {error}", temporary.display()))?;
    let mut zip = ZipWriter::new(file);
    let options = FileOptions::default()
        .compression_method(zip::CompressionMethod::Deflated)
        .last_modified_time(zip::DateTime::default());
    let base = root.join(relative_dir);

    let result = add_directory_to_zip(&mut zip, options, &base, archive_prefix).and_then(|()| {
        zip.finish()
            .map_err(|error| format!("finish {}: {error}", temporary.display()))
            .map(|_| ())
    });
    if let Err(error) = result {
        let _ = fs::remove_file(&temporary);
        return Err(error);
    }

    replace_file(&temporary, destination)
}

pub(crate) fn replace_file(temporary: &Path, destination: &Path) -> Result<(), String> {
    #[cfg(windows)]
    if destination.exists() {
        fs::remove_file(destination)
            .map_err(|error| format!("replace {}: {error}", destination.display()))?;
    }
    fs::rename(temporary, destination).map_err(|error| {
        format!(
            "rename {} to {}: {error}",
            temporary.display(),
            destination.display()
        )
    })
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
    zip.write_all(&contents)
        .map_err(|error| format!("write package entry: {error}"))
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
            add_file_to_zip(zip, options, &archive_path, &path)?;
        }
    }
    Ok(())
}

pub(crate) fn sha256_line(path: &Path) -> Result<String, String> {
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
