# Release Runbook

Keep `0.1.0` as the first release authority until the release is intentionally
published. Versions must agree across `VERSION`, Cargo metadata, plugin
manifests, marketplaces, citation metadata, changelog, documentation, and
archive names.

Run the gates from a clean checkout:

```bash
cargo toen verify
cargo toen test
cargo toen package --version 0.1.0
```

Packaging requires the reviewed benchmark evidence for the release and
produces exactly six files in `dist/`:

```text
toen-skill-v0.1.0.zip
toen-codex-plugin-v0.1.0.zip
toen-claude-code-plugin-v0.1.0.zip
toen-benchmark-evidence-v0.1.0.zip
toen-benchmark-report-v0.1.0.md
toen-v0.1.0-checksums.txt
```

Only these six owned paths are replaced. Existing unrelated files and
directories in `dist/` are preserved.

Each archive uses lexical entries, fixed timestamps, normalized text, no
symlinks or traversal paths, and includes the applicable README and attribution
files. The checksum file contains lowercase SHA-256 lines sorted by filename.
Repeat packaging and compare the archives byte-for-byte before publishing.
