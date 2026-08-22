# Toen

[![Main](https://github.com/airscripts/toen/actions/workflows/main.yml/badge.svg)](https://github.com/airscripts/toen/actions/workflows/main.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Corpus](https://img.shields.io/badge/corpus-500%20records-blue)](docs/methodology.md)

Token-efficient, source-grounded Livornese for Codex.

Toen is an optional Codex plugin that makes visible replies, status updates,
and tool narration shorter in contemporary Livornese. It is useful first and
playful second: technical terms and protected literals stay exact, while
ordinary prose can use a compact, documented local style.

Toen is off by default and explicit-only. It is text-only and Codex-only. It
does not provide a CLI runtime, hosted service, account, telemetry, hook, or
MCP server, and it makes no claims about hidden model reasoning.

## Installation

Pin the release and start a new Codex session after installation:

```bash
codex plugin marketplace add airscripts/toen --ref v0.1.0
codex plugin add toen --marketplace toen
```

The repository marketplace is `.agents/plugins/marketplace.json` and the
plugin is under `plugins/toen/`. A raw `toen-skill-v0.1.0.zip` is also produced
for direct skill installation. All release archives and checksums are also
published on the [GitHub Releases page](https://github.com/airscripts/toen/releases).
See [Installation](docs/installation.md).

## Quick Start

```text
$toen
$toen ammodino
$toen arranda
$toen de
$toen spengi
$toen ammodino explain this error
$toen spengi summarize this in normal Italian
```

`ammodino` is readable and concise for Italian readers. `arranda` is denser
and more local-first. Mode state lasts only for the current conversation;
new sessions start `spento`, while resume and compaction preserve the mode.
The status command is ASCII `de`; prose writes the Livornese interjection
`dé`.

See the [command reference](docs/commands.md), [house orthography](docs/orthography.md),
[privacy and limitations](docs/privacy.md), and the [full documentation index](docs/README.md).

## Maintainer Workflow

`toenctl` is a maintainer binary, not an end-user dependency. The Makefile is
the convenient project surface:

```bash
make verify
make test
make smoke-check
# Live, token-spending development campaign:
make smoke
make package VERSION=0.1.0
```

Install Lefthook once after cloning, then commits run the verification and
test commands automatically:

```bash
lefthook install
```

The pre-commit hook intentionally runs only `make verify` and `make test`,
with command output streamed while they run. Container, package, benchmark,
and release commands remain explicit. `make test` runs the unit and integration
suites and enforces at least 81% line coverage.

## Container Workflow

The checked-in [Containerfile](Containerfile) provides a disposable Linux
runtime with Rust 1.89, the pinned formatting and lint components, Make,
Python, curl, and archive tools. Use it when you want the same Linux tool
boundary as CI:

```bash
make container-verify
make container-test
make container-package VERSION=0.1.0
```

Each target builds the image and runs a fresh container with `--rm`. Package
archives are mounted back into `dist/`; the container itself is discarded.
Set `CONTAINER_ENGINE` for a compatible engine such as Podman, or override
`CONTAINER_IMAGE` to choose a local tag. macOS and Windows CI jobs remain
native so platform-specific behavior is still covered.

The individual commands are:

```bash
cargo run --release --locked --bin toenctl -- corpus check
cargo run --release --locked --bin toenctl -- sources verify --metadata-only
cargo run --release --locked --bin toenctl -- generate --check
cargo run --release --locked --bin toenctl -- bench smoke --check
cargo run --release --locked --bin toenctl -- bench run --release 0.1.0 --resume
cargo run --release --locked --bin toenctl -- bench judge --release 0.1.0
cargo run --release --locked --bin toenctl -- bench report --release 0.1.0
cargo run --release --locked --bin toenctl -- package --version 0.1.0
```

`corpus check` validates exactly 500 accepted TOML records, source URL
consistency, review metadata, variants, modes, and the 50/30 runtime core.
`generate --check` prevents drift in the skill, generated dictionary, source
notice, and token-budget manifest. `package` refuses to run until the complete
campaign and every release gate pass. It then creates deterministic plugin,
raw-skill, and benchmark-evidence ZIPs, the benchmark report, and SHA-256
checksums.

## Repository Map

| Path | Responsibility |
| --- | --- |
| `plugins/toen/` | Distributable Codex plugin and generated skill. |
| `corpus/accepted/` | One TOML file per accepted linguistic record. |
| `corpus/sources.toml` | Bibliography and local-attestation metadata. |
| `toenctl/` | Rust 2024 maintainer tooling. |
| `benchmarks/` | Protocol and saved development campaign artifacts. |
| `docs/` | English and Italian product and maintainer documentation. |
| `.github/workflows/` | Cross-platform verification, testing, build, and release workflows. |

## Evaluation and Limitations

The benchmark protocol compares normal Italian, terse Italian, Ammodino, and
Arranda on the specified Codex models. `bench smoke` runs the live Luna suite;
`bench smoke --check` is the non-spending CI check. Complete campaigns,
judging, and reports are manual, resumable, source-controlled release evidence.
See [Benchmarks](docs/benchmarks.md).

The accepted corpus contains 500 reviewed, locator-backed records. Release
quality still depends on passing the published behavioral and statistical
gates; the package command enforces that boundary.

## Contributing and Security

Read [CONTRIBUTING.md](CONTRIBUTING.md), run `make verify`, and update docs with
behavior changes. Report security issues privately as described in
[SECURITY.md](SECURITY.md).

## Acknowledgements

Thanks to Dario Moccia for inspiring this project.

## License

Rust code, plugin metadata, and skill instructions are MIT-licensed. Original
corpus records and generated linguistic documentation are licensed under
[CC BY 4.0](CORPUS-LICENSE.md). Third-party source material is not copied into
the repository. Plugin and raw-skill distributions carry both license files
and the generated source notice beside the runtime skill.
