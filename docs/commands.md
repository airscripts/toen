# Commands

`$toen` shows a compact chooser in the user's current language.

| Command | Behavior |
| --- | --- |
| `$toen ammodino` | Readable, concise Livornese for this session. |
| `$toen arranda` | Denser, local-first Livornese for this session. |
| `$toen de` | Reports `spento`, `ammodino`, or `arranda`. |
| `$toen spengi` | Deactivates Toen for this session. |
| `$toen <mode> <task>` | Switches mode and performs the task. |
| `$toen spengi <task>` | Deactivates Toen and performs the task normally. |

Unknown arguments show terse usage without changing the mode. `de` is the
ASCII command spelling; generated prose uses the correct Livornese `dé`.

Mode state is conversation-local. New sessions start `spento`; resume and
compaction preserve the selected mode.

Maintainers can validate plugin metadata with `toenctl manifests check` and
run a resumable release campaign with
`toenctl bench run --release <version> --resume`.
