# Corpus Methodology

The accepted corpus contains exactly 500 contemporary records, one TOML file
per record. Every record has a stable ID, form and lemma, Italian and English
glosses, grammatical metadata, variants, mode eligibility, an original example
with Italian gloss, precise evidence, and maintainer review metadata.

Livorno-specific attestation is required. General Tuscan sources can support a
record but cannot qualify it alone. Historical, disputed, and research-tier
records may be added outside `corpus/accepted`; they never enter generated
runtime output. Third-party pages are distilled, not copied.

The runtime core contains 50 Ammodino forms, 30 Arranda additions, and 12
grammar/orthography rules. The full corpus never enters a normal prompt.
Every compiled grammar rule records bibliography source IDs and is validated
against the same source catalog as accepted records.

`toenctl corpus check` enforces calendar-valid evidence and review dates,
source URL consistency, Livorno-specific qualifying evidence, enum values,
stable file/record IDs, alias collisions, command forms, and contiguous runtime
priorities. Generated documentation labels general Tuscan references as
supporting sources rather than local attestations.
