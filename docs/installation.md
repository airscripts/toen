# Installation

Toen is an optional Codex plugin. It has no CLI runtime, account, service,
telemetry, hook, or MCP server.

Pin the first release:

```text
codex plugin marketplace add airscripts/toen --ref v0.1.0
codex plugin add toen --marketplace toen
```

Start a new Codex session after installation. The plugin is explicit-only:
installation does not activate it. Maintainers can also distribute
`toen-skill-v0.1.0.zip` for direct skill installation.

The plugin archive, raw skill archive, benchmark-evidence archive, benchmark
report, and SHA-256 checksums are published as assets on the [GitHub Releases
page](https://github.com/airscripts/toen/releases). Release tags automatically
run verification, tests, gated packaging, checksum validation, and release
publication. A release cannot be packaged without complete passing evidence
for that exact version.

The repository marketplace is `.agents/plugins/marketplace.json`; the plugin
manifest is `plugins/toen/.codex-plugin/plugin.json`.

Both installation formats include the MIT software license, CC BY 4.0 corpus
license, and generated source notice beside the skill. Those attribution files
are distribution metadata and do not enter normal prompts.
