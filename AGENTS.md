# AGENTS.md

## Overview

Toen is a Codex plugin with a portable Markdown skill and a Rust 2024
maintainer workspace. The plugin and skill are explicit-only and have no
runtime service, account, telemetry, hook, or MCP server. `toenctl` validates
the corpus, renders committed assets, runs resumable benchmark campaigns,
judges and reports results, and packages gated releases.

## Boundaries

- `plugins/toen/` contains the distributable plugin and generated skill.
- `corpus/accepted/` is the exact 500-record accepted source of truth.
- `corpus/sources.toml` contains bibliography metadata; source pages are never
  copied into the repository.
- `toenctl/` owns maintainer validation, generation, benchmark orchestration
  boundaries, and packaging.
- `docs/` must change with product, command, corpus, or benchmark behavior.

## Style

Follow the Gitfleet house style: Rust edition 2024, four-space indentation,
100-column formatting, grouped imports, breathing room between logical phases,
thin command surfaces, and typed errors at expected boundaries. Human output
is concise and written to stdout; failures go to stderr. No legacy aliases.

Use Title Case for document titles, section headings, subtitles, and named
product concepts when they function as labels. Use sentence case for ordinary
prose, table cells, comments, and explanatory paragraphs. Keep code identifiers
in the language's idiomatic case and avoid decorative capitalization.

Write comments only when they explain a non-obvious invariant, safety boundary,
or deliberate tradeoff. Prefer self-documenting names and tests over comments
that restate the implementation.

## Testing Gates

The Lefthook pre-commit hook runs only `make verify` and `make test`. Keep
container, package, source-network, and live benchmark operations explicit.

```bash
CARGO_BUILD_JOBS=4 cargo fmt --check
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets --locked -- -D warnings
CARGO_BUILD_JOBS=4 cargo check --workspace --locked
CARGO_BUILD_JOBS=4 cargo test --workspace --all-targets --locked
CARGO_BUILD_JOBS=4 cargo llvm-cov --workspace --all-targets --locked --fail-under-lines 81
cargo run --release --locked --bin toenctl -- corpus check
cargo run --release --locked --bin toenctl -- generate --check
cargo run --release --locked --bin toenctl -- bench smoke --check
make container-verify
```

CI never spends model tokens. Live benchmark campaigns are manual, resumable,
and must publish prompts, fixtures, raw outputs, provider usage, randomized
judge inputs, rubrics, and reports.

Workspace line coverage must remain at or above 81%, keeping the enforced gate
strictly above 80%. Add meaningful unit or integration coverage for new paths;
do not exclude production modules merely to satisfy the threshold.

`make smoke` is a live, token-spending campaign. Use `make smoke-check` for
local validation and every automated workflow.

Container changes must keep `Containerfile`, `.dockerignore`, the disposable
Make targets, and container workflow coverage aligned. Keep Linux CI inside
fresh Rust 1.89 containers pinned by digest; retain native macOS and Windows
jobs for platform coverage. Do not put credentials, source pages, generated
archives, or model outputs into the image. `container-package` mounts tracked
release evidence read-only instead of copying it into an image layer.

## Release Rules

Keep changes unstaged. Do not commit, tag, push, publish, change remotes, or
delete releases. Versions must agree across `VERSION`, Cargo manifests,
plugin metadata, changelog, docs, and release archives.
