use std::env;
use std::fs;
use std::path::{Path, PathBuf};

use crate::ToenError;

#[derive(Clone, Debug)]
pub(crate) struct Workspace {
    root: PathBuf,
}

impl Workspace {
    pub(crate) fn discover(explicit: Option<&Path>) -> Result<Self, ToenError> {
        if let Some(root) = explicit {
            return Self::from_path(root);
        }
        if let Some(root) = env::var_os("TOEN_WORKSPACE") {
            return Self::from_path(Path::new(&root));
        }

        let mut current = env::current_dir()
            .map_err(|error| ToenError::Workspace(format!("current directory: {error}")))?;
        loop {
            if current.join("Cargo.toml").is_file()
                && current.join("VERSION").is_file()
                && current.join("corpus/accepted").is_dir()
            {
                return Ok(Self { root: current });
            }
            if !current.pop() {
                break;
            }
        }

        Err(ToenError::Workspace(
            "could not find a Toen workspace; run from the repository or set TOEN_WORKSPACE / --workspace <path>".to_owned(),
        ))
    }

    fn from_path(root: &Path) -> Result<Self, ToenError> {
        let root = fs::canonicalize(root)
            .map(normalize_windows_path)
            .map_err(|error| {
                ToenError::Workspace(format!(
                    "workspace {} is not readable: {error}",
                    root.display()
                ))
            })?;
        if !root.join("Cargo.toml").is_file()
            || !root.join("VERSION").is_file()
            || !root.join("corpus/accepted").is_dir()
        {
            return Err(ToenError::Workspace(format!(
                "workspace {} must contain Cargo.toml, VERSION, and corpus/accepted",
                root.display()
            )));
        }
        Ok(Self { root })
    }

    pub(crate) fn root(&self) -> &Path {
        &self.root
    }
}

#[cfg(windows)]
fn normalize_windows_path(path: PathBuf) -> PathBuf {
    let path = path.to_string_lossy().into_owned();
    if let Some(path) = path.strip_prefix("\\\\?\\UNC\\") {
        return PathBuf::from(format!(r"\\{path}"));
    }

    if let Some(path) = path.strip_prefix("\\\\?\\") {
        return PathBuf::from(path);
    }
    PathBuf::from(path)
}

#[cfg(not(windows))]
fn normalize_windows_path(path: PathBuf) -> PathBuf {
    path
}
