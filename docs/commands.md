# Commands

## Assistant Commands

| Command | Behavior |
| --- | --- |
| `$toen` | Show a compact chooser in the user's language. |
| `$toen ammodino` | Activate readable, concise Livornese. |
| `$toen arranda` | Activate denser, local-first Livornese. |
| `$toen de` | Report `spento`, `ammodino`, or `arranda`. |
| `$toen spengi` | Deactivate Toen. |
| `$toen <mode> <task>` | Switch mode and perform the task. |
| `$toen spengi <task>` | Deactivate Toen and perform the task normally. |

Claude Code uses `/toen:toen [command] [task]` and maps the arguments to this
same protocol. Unknown arguments show usage without changing state. `de` is the
ASCII command spelling; generated prose uses `dé`.

## Maintainer Commands

```text
toenctl corpus check
toenctl sources verify [--metadata-only]
toenctl manifests check
toenctl generate [--check]
toenctl bench smoke|run|judge|report
toenctl toenizer count|compare|report
toenctl doctor
toenctl verify
toenctl test
toenctl version
toenctl package --version <version>
```

`bench smoke --check` is non-spending. The other live benchmark commands are
explicit, resumable operations that may invoke the configured model provider.

The equivalent portable form is `cargo toen <command>`. Use
`toenctl --workspace <path> <command>` or `TOEN_WORKSPACE` to select a
workspace when invoked from elsewhere.
