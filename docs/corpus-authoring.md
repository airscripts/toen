# Corpus Authoring

Add accepted records under `corpus/accepted/` one TOML file per stable `liv-####`
ID. Keep the exact record structure used by neighboring files: canonical form,
glosses, grammatical metadata, allowed modes, original examples, evidence
locators, and review metadata.

Use original examples and short evidence locators. Do not copy source pages.
Every accepted record needs at least one Livorno-specific attestation; general
Tuscan sources are supporting evidence only. Preserve existing IDs and runtime
priority continuity.

Run the non-network checks after an edit:

```bash
cargo toen corpus check
cargo toen sources verify --metadata-only
cargo toen generate
cargo toen generate --check
```

Generated dictionary, source notices, skills, budgets, and Toenizer reports are
not edited by hand.
