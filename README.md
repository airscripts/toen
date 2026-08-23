# Toen

[![Main](https://github.com/airscripts/toen/actions/workflows/main.yml/badge.svg)](https://github.com/airscripts/toen/actions/workflows/main.yml)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Corpus](https://img.shields.io/badge/corpus-500%20records-blue)](docs/methodology.md)

Token-efficient, source-grounded Livornese for AI assistants.

Toen is an optional Codex plugin with a portable Markdown skill that makes
visible replies, status updates, and tool narration shorter in contemporary
Livornese. It is useful first and playful second: technical terms and
protected literals stay exact, while ordinary prose can use a compact,
documented local style.

Toen is off by default and explicit-only. It is text-only and makes no claims
about hidden model reasoning. The Codex integration is a plugin; the generated
skill can be used by any assistant that supports Markdown skills or custom
instructions. Toen does not provide a CLI runtime, hosted service, account,
telemetry, hook, or MCP server.

## Contents

- [Installation](#installation)
- [Quick Start](#quick-start)
- [Maintainer](#maintainer)
- [Containers](#containers)
- [Repository Map](#repository-map)
- [Evaluation](#evaluation)
- [Limitations](#limitations)
- [Contributing](#contributing)
- [Security](#security)
- [Acknowledgements](#acknowledgements)
- [License](#license)

## Installation

### Codex Plugin

To test the current checkout, clone the repository and install its local
marketplace:

```bash
git clone https://github.com/airscripts/toen.git
cd toen
codex plugin marketplace add "$PWD"
codex plugin add toen --marketplace toen
```

Start a new Codex session after installation. The plugin is explicit-only, so
installation does not activate it. A published marketplace release is not
available yet; release installation instructions will be added when one is
deployed. See [Installation](docs/installation.md).

### Portable Skill

The portable skill is [SKILL.md](plugins/toen/skills/toen/SKILL.md). Upload it
or copy it into an assistant's custom-instructions or skills directory, then
invoke it explicitly with `$toen ammodino` or `$toen arranda`. The host
assistant controls the exact installation location; no Codex marketplace is
required.

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
[privacy](docs/privacy.md), and the [full documentation index](docs/README.md).

## Maintainer

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

## Containers

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
| `plugins/toen/` | Distributable Codex plugin and portable generated skill. |
| `corpus/accepted/` | One TOML file per accepted linguistic record. |
| `corpus/sources.toml` | Bibliography and local-attestation metadata. |
| `toenctl/` | Rust 2024 maintainer tooling. |
| `benchmarks/` | Protocol and saved development campaign artifacts. |
| `docs/` | English product and maintainer documentation. |
| `.github/workflows/` | Cross-platform verification, testing, build, and release workflows. |

## Evaluation

The benchmark protocol compares normal Italian, terse Italian, Ammodino, and
Arranda on the specified Codex models. `bench smoke` runs the live Luna suite;
`bench smoke --check` is the non-spending CI check. Complete campaigns,
judging, and reports are manual, resumable, source-controlled release evidence.
See [Benchmarks](docs/benchmarks.md).

## Limitations

The accepted corpus contains 500 reviewed, locator-backed records. Release
quality still depends on passing the published behavioral and statistical
gates; the package command enforces that boundary.

## Contributing

Read [CONTRIBUTING.md](CONTRIBUTING.md), run `make verify`, and update docs
with behavior changes.

## Security

Report security issues privately as described in [SECURITY.md](SECURITY.md).

## Acknowledgements

Thanks to Dario Moccia for inspiring this project.

## License

Rust code, plugin metadata, and skill instructions are MIT-licensed. Original
corpus records and generated linguistic documentation are licensed under
[CC BY 4.0](CORPUS-LICENSE.md). Third-party source material is not copied into
the repository. Plugin and raw-skill distributions carry both license files
and the generated source notice beside the runtime skill.
