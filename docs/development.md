# Development

Clone the repository and enter it before running the maintainer commands:

```bash
git clone https://github.com/airscripts/toen.git
cd toen
```

| Command | Purpose |
| --- | --- |
| `cargo toen verify` | Formatting, lint, compilation, corpus, sources, manifests, generation, and report checks. |
| `cargo toen test` | Workspace tests and the 81% line-coverage gate. |
| `cargo toen corpus check` | Validate accepted records and runtime core. |
| `cargo toen sources verify --metadata-only` | Validate bibliography without network access. |
| `cargo toen manifests check` | Validate all distribution and marketplace metadata. |
| `cargo toen generate` | Render committed generated files. |
| `cargo toen generate --check` | Fail if generated files drift. |
| `cargo toen toenizer report --check` | Fail if the deterministic report drifts. |
| `cargo toen bench smoke --check` | Validate the non-spending benchmark manifest. |
| `cargo toen bench smoke` | Run the explicit live smoke campaign and spend provider tokens. |
| `cargo toen package --version 0.1.0` | Build core files; include valid evidence when present. |
| `make -f Makefile.container verify` | Run verification in a disposable container. |

Lefthook runs only `make verify` and `make test` before a commit. Container,
package, live source-link, and live benchmark operations remain explicit. CI
does not spend model tokens; the live benchmark commands do.

Install the pinned coverage runner before `cargo toen test` when it is not
already available:

```bash
cargo install cargo-llvm-cov --version 0.8.7 --locked
```

Run `toenctl sources verify` without `--metadata-only` only when an explicit
network verification is desired.
