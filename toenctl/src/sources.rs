//! Bibliography parsing and optional HTTPS verification are maintainer operations.

use std::collections::HashMap;
use std::fs;
use std::path::Path;
use std::time::Duration;

use crate::{SourceMetadata, SourcesFile};

pub(crate) fn verify(root: &Path, option: Option<&str>) -> Result<(), String> {
    let metadata_only = option == Some("--metadata-only");

    if option.is_some() && !metadata_only {
        return Err("sources verify accepts only --metadata-only".to_owned());
    }

    let text = fs::read_to_string(root.join("corpus/sources.toml"))
        .map_err(|error| format!("read bibliography: {error}"))?;
    let sources: SourcesFile =
        toml::from_str(&text).map_err(|error| format!("parse bibliography: {error}"))?;

    validate_catalog(&sources)?;

    for source in &sources.source {
        if !metadata_only {
            println!("sources: checking live URL for {}", source.id);
            check_url(source.url.as_str())?;

            if let Some(archive_url) = &source.archive_url {
                println!("sources: checking archive URL for {}", source.id);
                check_url(archive_url.as_str())?;
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
    let config = ureq::Agent::config_builder()
        .timeout_global(Some(Duration::from_secs(15)))
        .https_only(true)
        .build();
    let agent = config.new_agent();
    let response = agent
        .get(url)
        .call()
        .map_err(|error| format!("check {url}: HTTP request failed: {error}"))?;
    if !(200..400).contains(&response.status().as_u16()) {
        return Err(format!("check {url}: HTTP status {}", response.status()));
    }
    Ok(())
}

pub(crate) fn metadata(root: &Path) -> Result<HashMap<String, SourceMetadata>, String> {
    let text = fs::read_to_string(root.join("corpus/sources.toml"))
        .map_err(|error| format!("read bibliography: {error}"))?;
    let sources: SourcesFile =
        toml::from_str(&text).map_err(|error| format!("parse bibliography: {error}"))?;

    validate_catalog(&sources)?;

    Ok(sources
        .source
        .into_iter()
        .map(|source| {
            (
                source.id,
                SourceMetadata {
                    url: source.url.0,
                    archive_url: source.archive_url.map(|url| url.0),
                    local_attestation: source.local_attestation,
                },
            )
        })
        .collect())
}

pub(crate) fn validate_catalog(sources: &SourcesFile) -> Result<(), String> {
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
            || !source.url.as_str().starts_with("https://")
            || source
                .archive_url
                .as_ref()
                .is_some_and(|url| !url.as_str().starts_with("https://"))
        {
            return Err(format!("source {} has incomplete metadata", source.id));
        }
    }

    if sources
        .source
        .iter()
        .filter(|source| source.local_attestation)
        .count()
        == 0
    {
        return Err("bibliography needs Livorno-specific sources".to_owned());
    }

    Ok(())
}
