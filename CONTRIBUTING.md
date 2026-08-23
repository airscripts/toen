# Contributing

Toen follows the repository conventions established by Gitfleet where they
apply: Rust 2024, four-space formatting, typed validation, concise human output,
and documentation changes in the same workflow as behavior changes.

## Before Opening A Change

Install Rust 1.89 from `rust-toolchain.toml`, then run:

```bash
cargo toen verify
cargo toen test
```

Install the repository hook once with `lefthook install`. The pre-commit hook
runs only `make verify` and `make test`; it does not build containers, verify
live source links, or call a model.

For the container boundary, run `make -f Makefile.container verify` when a
Docker- or Podman-compatible engine is available. CI keeps native platform
coverage in addition to the container gate.

## Corpus Changes

Accepted records live one per file under `corpus/accepted/`. Keep IDs stable,
write original examples, preserve precise source locators, and do not copy
substantial third-party text. A record needs Livorno-specific attestation;
general Tuscan evidence is supporting context only.

Corpus records and generated linguistic documentation are CC BY 4.0. Rust code,
plugin instructions, and maintainer tooling are MIT.

## Pull Requests

Explain the behavior change, validation performed, and documentation updated.
Keep changes focused and do not commit generated release archives under `dist/`.
