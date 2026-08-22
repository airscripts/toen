# Security

Toen is a local Codex skill. It does not run a service, collect telemetry,
process accounts, install hooks, or expose an MCP server.

Do not include credentials, private prompts, personal data, or unredacted model
outputs in issues or benchmark artifacts. Report suspected security problems
privately to the repository maintainers before public disclosure.

Network source verification and live benchmark campaigns are explicit
maintainer actions. CI uses metadata-only source checks and `bench smoke
--check`; it never invokes Codex or spends model tokens.
