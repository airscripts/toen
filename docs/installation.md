# Installation

Toen is an optional Codex plugin with a portable Markdown skill. It has no CLI
runtime, account, service, telemetry, hook, or MCP server.

## Codex Plugin

Clone the repository, then add its local marketplace and install the plugin:

```bash
git clone https://github.com/airscripts/toen.git
cd toen
codex plugin marketplace add "$PWD" --json
codex plugin add toen --marketplace toen
```

Start a new Codex session after installation. The plugin is explicit-only:
installation does not activate it. Maintainers can also distribute
the generated raw skill archive for direct skill installation.

## Portable Skill

The generated skill is [SKILL.md](../plugins/toen/skills/toen/SKILL.md). Upload
it or copy it into the custom-instructions or skills directory of any assistant
that supports Markdown skill files. Invoke it explicitly with `$toen ammodino`
or `$toen arranda`; the host assistant determines the exact installation
location.

## Published Releases

No public marketplace release has been deployed yet. Once a gated release is
published, this section will contain the pinned marketplace command and links
to its plugin, skill, benchmark-evidence, report, and checksum archives.

The repository marketplace is `.agents/plugins/marketplace.json`; the plugin
manifest is `plugins/toen/.codex-plugin/plugin.json`.

Both installation formats include the MIT software license, CC BY 4.0 corpus
license, and generated source notice beside the skill. Those attribution files
are distribution metadata and do not enter normal prompts.
