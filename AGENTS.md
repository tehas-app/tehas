# AGENTS.md

Rules for agents working in this repository. Read `README.md` for the map of
the repository and `host/docs/` for how tehas is operated.

## What this is

tehas is a software factory: GitHub issues go in, reviewed pull requests
come out, on self-hosted models. It is a single-project appliance. One host
serves one product repository; the host's settings are an instance directory.

This repository is also a product. A tehas instance on the host `tehas-tehas`
works on it, which is how you got here. You change the source; you do not
change the factory that is running you. A merged change reaches a host only
when a person pulls and runs `install.sh` there.

| Path | What it is |
| --- | --- |
| `bin/` | The two commands: `tehas` for a person, `tehasbot` for the units. Installed to `~/tehas/bin` |
| `libexec/` | The scripts behind them, bash and one Deno script. Installed to `~/tehas/libexec` |
| `lib/config.sh` | Loads and validates instance settings. Every command sources it |
| `host/` | Everything else that is installed: units, the docs server, the sandbox image, the manual in `host/docs/` |
| `install.sh`, `bootstrap.sh` | Install tehas on a host; prepare a host's dependencies |
| `instances/` | Instance directories: `example/` to copy, and the real ones |
| `test/` | Shell tests, one file per unit, helpers in `test/lib.sh` |
| `scripts/` | The gate (`verify.sh`) and the lint (`check-generic.sh`) |
| `docs/` | Records about this project: specs, plans, decisions, logs. Not installed |

## Hard rules

1. **Platform files name no product and no host.** A setting goes in the
   instance directory, never as a default in a script. `scripts/check-generic.sh`
   enforces a list of names; the rule is wider than the list. Examples in help
   text and the manual use the tehas-tehas instance.
2. **A command fails closed.** A missing or malformed setting stops it with
   exit 78 before it calls GitHub, Docker or git and before it writes anything.
   Nothing falls back to a default repository or a wider set of labellers.
3. **Loading settings has no side effects.** `lib/config.sh` runs no git
   command, makes no network call and writes nothing, and `tehas_load` never
   exits, because login shells source it.
4. **Do not widen what a cycle's container is given.** The mounts and
   environment variables of `libexec/work` are pinned by `test/work_test.sh`.
   No Docker socket, no SSH keys, nothing from `~/.config/tehas`. A change here
   is a security change: say so in the pull request and do not relax the test
   to make it pass.
5. **Authorisation is who applied the label.** The dispatcher reads the label
   events and compares the account with the allowed list. Do not replace that
   with a check that the label is present.
6. **Tests first.** Every behaviour change comes with a case in `test/`. Tests
   need bash, git, jq and shellcheck only: no network, no sandbox image, no
   product checkout. Stub the outside world with `t_stub`.
7. **Do not edit your own gates.** `.pi/`, `AGENTS.md`, `scripts/verify.sh` and
   `scripts/check-generic.sh` are changed by people, not by a work cycle. If a
   gate is wrong, say so in your handoff.
8. **Leave the real instances alone.** Every directory under `instances/`
   except `example/` is the settings of a running factory, and
   `instances/tehas-tehas/` is the one running you. Propose a change in the
   pull request text, do not make it.
9. **Nothing merges on its own.** Open a pull request and stop. Never push to
   the default branch.
10. **No secrets in the repository**, no tokens in command lines, logs or
    comments.

## Style

- Bash: `set -uo pipefail` (or `-euo` where the script already has it), two
  spaces, `shellcheck -x` clean with no blanket disables. A header comment says
  what the command does, its usage and its exit statuses. Comments explain why.
- Prose in comments, the manual and commit messages: plain and direct, short
  sentences, no em dashes, no hype.
- The manual in `host/docs/` describes what is true on a host today. When you
  change behaviour, change the page that describes it in the same commit.
  `host/docs/features.md` says how far each feature is proven; do not mark
  something proven that you have not seen work.

## Verify

```
scripts/verify.sh          # fast gate: lint, shellcheck, shell tests
scripts/verify.sh --full   # plus deno lint
```

Both print one line per step and exit non-zero on failure. Run the fast gate
before you say a change is done, and quote its output.

## Commits

Commit your work before you report a step as done: `git add` every file you
changed and `git commit`. Never finish with uncommitted changes in your
worktree. The driver's fallback for uncommitted work can drop a file, and a
change that is half committed fails the gate.

One logical change per commit. Subject in the imperative or as a short
statement of what changed, as the existing history does (`Settings library
with tests`, `install.sh installs by feature from an instance directory`). No
issue numbers in the subject; reference the issue in the pull request.
