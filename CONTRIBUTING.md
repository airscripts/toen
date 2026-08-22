# Contributing

Toen follows the repository conventions established by [Gitfleet](../gitfleet)
where they apply: Rust 2024, four-space formatting, typed validation, concise
human output, explicit structured artifacts, and documentation changes in the
same workflow as behavior changes.

Use Title Case for titles, subtitles, section headings, and named concepts.
Use sentence case for ordinary prose, comments, and table content. Keep code
identifiers idiomatic and avoid capitalization that does not convey meaning.

## Before Opening a Change

Install Rust 1.89 from `rust-toolchain.toml` and the pinned coverage runner,
then run:

```bash
cargo install cargo-llvm-cov --version 0.8.7 --locked
make verify
make test
```

The equivalent commands are listed in [the development guide](docs/development.md).
Install the repository hook once with `lefthook install`. The pre-commit hook
runs only `make verify` and `make test`, streaming their execution output; it
does not build containers, package archives, or spend benchmark tokens.
`make test` runs the unit and integration suites and fails below 81% line
coverage.
For the Linux container boundary, also run `make container-verify` when a
Docker- or Podman-compatible engine is available. CI runs Linux gates in
disposable containers and keeps macOS/Windows checks native.
CI never spends model tokens or makes live benchmark calls.

Completed release campaigns under `benchmarks/releases/<version>/` are
versioned evidence. Review them for credentials, personal data, and unrelated
content before committing them for a release tag.

## Corpus Changes

Accepted records live one-per-file under `corpus/accepted/`. Keep IDs stable,
write original examples, preserve precise source locators, and do not copy
substantial third-party text. A record needs Livorno-specific attestation;
general Tuscan evidence is supporting context only.

Corpus records and generated linguistic documentation are CC BY 4.0. Rust code,
plugin instructions, and maintainer tooling are MIT.

## Pull Requests

Explain the behavior change, validation performed, documentation updated, and
any benchmark evidence. Keep changes focused and do not commit generated
release archives under `dist/`. User-facing behavior changes must update the
English, Italian, and Livornese guides together.
