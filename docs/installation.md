# Installation

Toen is a local, explicit-only skill. It has no runtime service, account,
telemetry, hook, or MCP server.

## Portable Skill

```bash
git clone https://github.com/airscripts/toen.git
cd toen
```

Copy [skill/toen/SKILL.md](../skill/toen/SKILL.md) into the skill or
custom-instructions directory supported by your assistant. Activate it only
with the documented `$toen` command protocol.

## Codex Plugin

```bash
codex plugin marketplace add .
codex plugin add toen --marketplace toen
```

Start a new session after installation. This public repository marketplace is
the supported installation path until a hosted marketplace publication exists.

## Claude Code Plugin

```bash
claude plugin marketplace add .
claude plugin install toen@toen
claude plugin validate .
```

Invoke the namespaced skill explicitly with `/toen:toen [command] [task]`.
Its frontmatter disables implicit model invocation.

## Distribution Contents

The portable, Codex, and Claude Code directories are self-contained and each
includes a README, software license, corpus license, and source notice. See the
[Release Runbook](release.md) for the four core release files and optional
benchmark-evidence artifacts.
