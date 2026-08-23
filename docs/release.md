# Release Runbook

Keep `0.1.0` as the first release authority until the release is intentionally
published. Versions must agree across `VERSION`, Cargo metadata, plugin
manifests, marketplaces, citation metadata, changelog, documentation, and
archive names.

Keep the changelog in Keep a Changelog format with an `## [Unreleased]` section
followed by bracketed version sections such as `## [0.1.0] - 2026-08-23`.
The release workflow extracts the matching dated version section. Release tags
must be stable `vX.Y.Z` tags; prerelease tags are rejected until benchmark
evidence and archive naming support them consistently.

Run the gates from a clean checkout:

```bash
cargo toen verify
cargo toen test
cargo toen package --version 0.1.0
```

Packaging always produces four core files in `dist/`:

```text
toen-skill-v0.1.0.zip
toen-codex-plugin-v0.1.0.zip
toen-claude-code-plugin-v0.1.0.zip
toen-v0.1.0-checksums.txt
```

Live benchmark evidence is optional. When `benchmarks/releases/0.1.0/` is
absent, no model tokens or benchmark report are required. When the directory
exists, it must contain complete reviewed evidence that passes every release
gate, and packaging adds:

```text
toen-benchmark-evidence-v0.1.0.zip
toen-benchmark-report-v0.1.0.md
```

The six possible artifact paths are package-owned. A core-only package removes
stale benchmark artifacts from a previous run, while existing unrelated files
and directories in `dist/` are preserved.

Each archive uses lexical entries, fixed timestamps, normalized text, no
symlinks or traversal paths, and includes the applicable README and attribution
files. The checksum file contains lowercase SHA-256 lines for the artifacts
present, sorted by filename. Repeat packaging and compare the archives
byte-for-byte before publishing.
