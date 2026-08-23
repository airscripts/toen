# Tokenization Methodology

Toenizer is a deterministic local estimator. Its default and currently only
engine is `o200k-base`, exposed as an implementation identifier rather than a
universal token standard.

## Metrics

For exact input, Toenizer reports token estimate, UTF-8 byte length, and line
count. `compare` computes signed token difference as baseline minus candidate and
`100 × (baseline - candidate) / baseline`. A zero baseline produces `null` in
JSON and `n/a` in human output. Negative estimated savings are valid increases.

Toenizer performs no Unicode normalization, rewriting, provider request, or
billing calculation. It cannot make Claude-specific tokenizer claims and never
substitutes for provider-reported usage or quality evaluation.

## Corpus Report

`toenctl toenizer report` measures every Italian/Livornese corpus example, emits
aggregate totals and paired median saving, and records shorter/equal/longer
counts. The report also records size, hash, and budget utilization for portable,
Codex, and Claude Code variants. Corpus savings are informational; releases gate
determinism and size budgets, not a minimum saving percentage.
