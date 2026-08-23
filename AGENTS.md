# AGENTS.md

## Overview

Toen is a text-only, explicit-only assistant skill with certified Codex and
Claude Code integrations. It has no runtime service, account, telemetry, hook,
or MCP server. The Rust 2024 maintainer workspace validates the corpus,
renders committed assets, runs explicit non-spending benchmark checks and
manual resumable live campaigns, estimates local token sizes, and packages
gated releases.

## Boundaries

- `skill/toen/` is the host-neutral generated skill distribution.
- `plugins/codex/toen/` and `plugins/claude-code/toen/` are self-contained host
  integrations generated from the same renderer.
- `corpus/accepted/` is the exact 500-record accepted source of truth.
- `corpus/sources.toml` contains bibliography metadata; source pages are never
  copied into the repository.
- `toenctl/` owns validation, generation, Toenizer, benchmark orchestration,
  workspace discovery, and packaging.
- `benchmarks/` contains committed scenarios, fixtures, schemas, rubrics, and
  versioned release evidence; live campaign runs are explicit and resumable.
- `docs/` changes with product, command, corpus, benchmark, or packaging behavior.

## Style

Follow the Gitfleet house style: Rust edition 2024, four-space indentation,
100-column formatting, grouped imports, breathing room between logical phases,
thin command surfaces, and typed errors at expected boundaries. Human output
is concise and written to stdout; failures go to stderr. No legacy aliases.

Use Title Case for document titles, section headings, subtitles, and named
product concepts when they function as labels. Use sentence case for ordinary
prose, table cells, comments, and explanatory paragraphs. Keep code identifiers
in the language's idiomatic case.

Write comments only when they explain a non-obvious invariant, safety boundary,
or deliberate tradeoff. Prefer self-documenting names and tests.

## Testing Gates

The Lefthook pre-commit hook runs only `make verify` and `make test`. The
commands are also available through the portable Cargo alias:

```bash
cargo toen verify
cargo toen test
cargo toen toenizer report --check
cargo toen bench smoke --check
```

Verification is non-spending and covers formatting, Clippy, compilation,
corpus and source metadata, manifests, generated drift, the smoke manifest, and
Toenizer reports. Live benchmark campaigns are manual, resumable, and publish
prompts, fixtures, raw outputs, provider usage, randomized judge inputs,
rubrics, and reports. `sources verify` without `--metadata-only` and live
benchmark commands are explicit network operations.

Workspace line coverage must remain at or above 81%, keeping the enforced gate
strictly above 80%. Add meaningful unit or integration coverage for new paths;
do not exclude production modules merely to satisfy the threshold.

Keep CI inside fresh Rust 1.89 containers pinned by digest. Retain native
Windows x86-64 and ARM64, macOS x86-64 and ARM64, and Linux x86-64 and ARM64
build/test coverage. Do not
put credentials, source pages, generated archives, benchmark outputs, or model
outputs into the image. `make -f Makefile.container package` mounts tracked
release evidence read-only.

## Release Rules

Keep changes unstaged. Do not commit, tag, push, publish, change remotes, or
delete releases. Versions must agree across `VERSION`, Cargo manifests,
plugin metadata, marketplace entries, citation metadata, changelog, docs, and
release archives.

`toenctl package --version 0.1.0` requires the complete reviewed benchmark
evidence set and produces exactly five release files plus one checksum file:

- `toen-skill-v0.1.0.zip`
- `toen-codex-plugin-v0.1.0.zip`
- `toen-claude-code-plugin-v0.1.0.zip`
- `toen-benchmark-evidence-v0.1.0.zip`
- `toen-benchmark-report-v0.1.0.md`
- `toen-v0.1.0-checksums.txt`

Packaging replaces only those six owned paths in `dist/`; unrelated files and
directories remain untouched.
