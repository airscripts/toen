# Development

The Makefile is the supported convenience surface:

| Target | Purpose |
| --- | --- |
| `make verify` | Formatting, lint, compilation, corpus, sources, manifests, and generated-file checks. |
| `make test` | Run all workspace unit and integration tests, enforcing at least 81% line coverage. |
| `make corpus` | Validate the exact accepted-record count and runtime core. |
| `make sources` | Validate bibliography metadata without network access. |
| `make manifests` | Validate plugin JSON, skill policy, and marketplace metadata. |
| `make generate` | Render committed skill and generated documentation. |
| `make generate-check` | Fail if generated assets drift. |
| `make smoke-check` | Validate the non-spending smoke manifest used by CI. |
| `make smoke` | Run the live, token-spending 12-scenario Luna campaign. |
| `make package` | Build five gated release files after the complete benchmark passes. |
| `make container-verify` | Run the full verification gate in a disposable Linux container. |
| `make container-test` | Run workspace tests in a disposable Linux container. |
| `make container-package` | Package releases in a disposable container and write archives to `dist/`. |

## Git Hooks

Toen uses [Lefthook](https://github.com/evilmartians/lefthook) for the local
pre-commit hook. Install it once after cloning:

```bash
lefthook install
```

The hook runs exactly `make verify` and `make test`, and Lefthook streams the
execution and failure output while they run. It does not run container,
package, source-network, or live benchmark commands.

Install the pinned coverage runner before using `make test` on the host:

```bash
cargo install cargo-llvm-cov --version 0.8.7 --locked
```

For a live source-link check, run `toenctl sources verify` without
`--metadata-only`; it requires network access and is intentionally not a CI
gate. Live benchmark campaigns are manual and never run in CI.

Use `make smoke-check` for automation. `make smoke` invokes Codex and spends
model tokens; it deliberately remains outside `verify`, tests, Lefthook, and
CI.

The container targets use `Containerfile` and default to Docker. Set
`CONTAINER_ENGINE=podman` or `CONTAINER_IMAGE=<tag>` when a different local
runtime or image tag is needed. The image uses Rust 1.89 Bookworm and copies
the checkout into `/workspace`; each run is removed after completion.

## Release Workflow

Push a `v<version>` tag, or manually dispatch the Release workflow with a tag.
The workflow checks that the tag matches `VERSION` and `CHANGELOG.md`, then
runs verification and tests against that tag. Packaging runs in a disposable
Rust 1.89 container and requires the complete passing campaign for the same
version. Checksum validation consumes the uploaded files, and the final job
publishes the plugin ZIP, raw skill ZIP, benchmark-evidence ZIP, benchmark
report, and checksum file as a GitHub Release.

The Linux test workflow also installs the pinned official Codex CLI into an
isolated temporary `CODEX_HOME`, adds the checked-out marketplace, installs
Toen, and lists the installed plugin. This exercises installation without
authentication or a model call.

The pinned toolchain is Rust 1.89. `Cargo.lock` is committed so CI and release
builds use the same dependency resolution. Linux coverage uses the pinned
`cargo-llvm-cov` 0.8.7 release and fails below 81% line coverage.
