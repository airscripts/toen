# Changelog

All notable Toen changes are documented here using Keep a Changelog and
Semantic Versioning.

## [Unreleased]

## [0.1.0] - 2026-08-23

### Added

- Initial text-only release of Toen as a portable Markdown skill, Codex plugin,
  and Claude Code plugin generated from one linguistic source of truth.
- Explicit `ammodino` and `arranda` modes, state inspection with `de`, and
  deactivation with `spengi`, including conversation-local resume and
  compaction behavior.
- Protected-text rules that preserve code, commands, paths, URLs, identifiers,
  logs, errors, quotations, numbers, and requested output formats.
- A reviewed corpus of exactly 500 Livornese records with stable identities,
  linguistic metadata, original examples, evidence locators, review data, and
  eight bibliography entries.
- Self-contained Codex and Claude Code marketplace distributions with host-native
  invocation metadata, installation validation, licensing, and source notices.
- The Rust 2024 `toenctl` maintainer workspace for corpus, source, manifest, and
  workspace validation; deterministic generation; diagnostics; and atomic file
  updates.
- Toenizer commands for exact text and file measurement, baseline comparison,
  and reproducible reports using the disclosed `o200k-base` tokenizer.
- Resumable four-condition benchmark campaigns with multilingual prompts,
  isolated fixtures, provider usage, randomized blind judging, compatibility
  transcripts, statistical reports, and enforced release gates.
- Reproducible packaging for the portable skill, both plugins, reviewed benchmark
  evidence, benchmark reports, and sorted SHA-256 checksums.
- Rollback-safe replacement of the six owned release files while preserving
  unrelated `dist/` contents and rejecting incomplete or unreviewed evidence.
- Pinned container and cross-platform CI coverage for Linux, macOS, and Windows
  on x86-64 and ARM64, with formatting, linting, tests, coverage, dependency
  policy, vulnerability auditing, and non-spending smoke checks.
- Stable-tag release automation with validated dated notes, artifact provenance
  attestations, tag verification, and prerelease rejection until complete
  prerelease evidence is supported.
- Product, architecture, command, installation, corpus, benchmark, tokenization,
  privacy, security, support, contribution, container, and release documentation.

### Security

- Kept Toen local and explicit-only, with no runtime service, account, telemetry,
  hook, MCP server, dynamic vocabulary fetch, or model-calling runtime.
- Kept CI non-spending and isolated live source checks and benchmark campaigns
  behind explicit maintainer commands.
- Excluded credentials, caches, build output, model output, and release evidence
  from container image layers while mounting reviewed packaging evidence
  read-only.
- Pinned GitHub Actions, Rust container images, host CLI dependencies, and audit
  tooling for reproducible automation and dependency review.
