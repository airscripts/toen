# Architecture

Toen has one linguistic source of truth and three generated distributions.

```mermaid
flowchart LR
    corpus[corpus/accepted and grammar.toml] --> toenctl[toenctl]
    toenctl --> skill[skill/toen]
    toenctl --> codex[plugins/codex/toen]
    toenctl --> claude[plugins/claude-code/toen]
    toenctl --> generated[docs and schemas]
    generated --> package[deterministic ZIPs]
```

`toenctl` discovers the workspace, validates typed corpus relationships,
renders the canonical skill and host frontmatter, estimates sizes locally, and
writes generated files through temporary files followed by rename. Every host
package is self-contained because installed plugins cannot rely on repository
paths outside their archive.

The host integrations differ only at the boundary: Codex exposes `$toen` and
its explicit invocation policy, while Claude Code exposes the namespaced
`/toen:toen` skill with `disable-model-invocation: true`.
