# Ticket Concurrency Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A host works on up to `TEHAS_TICKET_CONCURRENCY` issues side by side, 1 by default, and the console has a tab per cycle.

**Architecture:** `libexec/dispatch` becomes a short poll that marks issues and starts one systemd unit per cycle. A new `libexec/cycle` runs `tehas work` for one issue and reports it, holding a lock the poll counts. Each cycle gets a workspace and a transcripts directory of its own, mounted at the same path in every container, and writes its own live file; `live/running.json` is assembled from those.

**Tech Stack:** bash, jq, flock, git, systemd user units, plain JavaScript in the pages, `node --test` for the page logic, one Rust test in `console/`.

**Spec:** `docs/specs/2026-10-10-ticket-concurrency.md`. Read it first. Where this plan and the spec differ, stop and say so.

## Global Constraints

- Branch `ticket-concurrency`. Never push to `main`. Open a pull request and stop.
- `AGENTS.md` applies in full. Do not edit `.pi/`, `AGENTS.md`, `scripts/verify.sh`, `scripts/check-generic.sh` or anything under `instances/` except `instances/example/`.
- Platform files name no product and no host. Examples in help text and the manual use the tehas-tehas instance.
- A command fails closed: a missing or malformed setting is exit 78 before GitHub, Docker or git is called and before anything is written.
- `lib/config.sh` runs no git command, makes no network call and writes nothing. `tehas_load` never exits.
- The mounts and environment of a cycle's container are pinned by `test/work_test.sh`. Task 3 changes two mount sources. Pin the new values exactly. Do not relax the test. The pull request text says it is a security change.
- Tests first, in `test/`, with bash, git, jq, shellcheck and flock only. No network. Stub the outside world with `t_stub`.
- Bash: `set -uo pipefail`, two spaces, `shellcheck -x` clean with no blanket disables. A header comment gives what the command does, its usage and its exit statuses.
- Prose in comments, the manual and commit messages: plain, short sentences, no em dashes. Comments are terse and say why.
- The manual in `host/docs/` changes in the commit that changes the behaviour. `features.md` does not call anything proven that has not been seen on a host.
- One commit per task. Subjects are short statements, with no issue numbers and no prefixes.
- Before every commit run `scripts/verify.sh` and read its output. Tasks 4 and 5 also run `scripts/verify.sh --full`.
- With `TEHAS_TICKET_CONCURRENCY` unset, a host behaves as it does before this plan.

## Review Focus

Each line has a test in the task named.

1. **A cycle that ends within seconds** (bad settings, a failed push check). The poll must not report it as "did not start" on top of the cycle's own report. Task 2, `dispatch_test.sh`, "a cycle that ended at once still counts as started".
2. **A start marker left by an earlier cycle of the same issue.** The poll must not take it for this start. Task 2, `dispatch_test.sh`, "a start marker left by an earlier cycle does not count".
3. **`live/running.json` in the old shape**, which a host has between the upgrade and its next cycle. Health and both pages must read it. Task 4, `status_test.sh`; Task 5, `console_tabs_test.mjs`.
4. **A cycle killed without a word.** Its live file says running for ever. Task 4, `status_test.sh`, "a cycle that stopped writing is lost".
5. **`tehasbot cycle` with something that is not an issue number**, as `systemctl --user start tehas-cycle@x` gives it. Exit 64, nothing touched. Task 2, `cycle_test.sh`.

## File Structure

| File | Task | Responsibility |
| --- | --- | --- |
| `lib/config.sh` | 1, 2 | The setting, each cycle's share, the test hooks kept from the environment |
| `lib/cycles.sh` (new) | 2 | Which cycles are running, from their locks |
| `libexec/dispatch` | 2, 6 | The poll: rules, slots, start the units |
| `libexec/cycle` (new) | 2, 3 | One cycle for one issue, and its report |
| `host/systemd/tehas-cycle@.service` (new) | 2 | The unit a cycle runs as |
| `libexec/index` | 2 | Refuses while a cycle runs |
| `lib/sandbox.sh`, `libexec/work` | 1, 3, 4 | The share, the per-issue workspace and transcripts, the live file |
| `lib/live.sh` (new) | 4 | Assembles `live/running.json` |
| `libexec/health`, `libexec/summary`, `libexec/dev` | 4 | Read the new shape |
| `host/fun/console-tabs.js` (new) | 5 | Which tabs there are and which is shown; draws the row |
| `host/fun/game-view.js`, `host/fun/index.html`, `host/fun/game-view.css` | 5 | The factory floor's console |
| `host/docs/dashboard.frag`, `host/fun/manual.css` | 5 | The plain dashboard's console |
| `install.sh`, `bin/tehasbot` | 2, 4 | Install the new files; the `cycle` subcommand |

---

### Task 1: The setting and each cycle's share

**Files:**
- Modify: `lib/config.sh`
- Modify: `libexec/work` (the `docker run` of the cycle, and the header)
- Modify: `host/docs/tools.md`, `instances/example/instance.env`
- Test: `test/config_test.sh`, `test/work_test.sh`

**Interfaces:**
- Produces: exported `TEHAS_TICKET_CONCURRENCY` (whole number, default `1`), `TEHAS_CYCLE_CPUS` and `TEHAS_CYCLE_MEMORY` (what one cycle's container gets). With the setting at 1 the two are `TEHAS_WORK_CPUS` and `TEHAS_WORK_MEMORY` as written.

- [ ] **Step 1: Write the failing tests in `test/config_test.sh`**

Add `"TEHAS_TICKET_CONCURRENCY=0"` and `"TEHAS_TICKET_CONCURRENCY=two"` to the list of the loop `for bad in "TEHAS_WORK_MEMORY=lots" ...` (the one whose case is named `loading rejects $bad`).

Add these cases just before the final `t_done`:

```bash
# The host's CPU count is read with nproc; a stub makes the split the same on
# every machine. The memory values are far below what any test host has.
t_case "one ticket at a time by default, and the cycle gets the budget as written"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_CPUS=1 TEHAS_WORK_MEMORY=512m
t_run "$load && echo \"\$TEHAS_TICKET_CONCURRENCY \$TEHAS_CYCLE_CPUS \$TEHAS_CYCLE_MEMORY\""
t_eq "$t_rc" 0
t_eq "$t_out" "1 1 512m"

t_stub nproc 'echo 8'
t_case "the budget is divided between the tickets, rounded down"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=3 TEHAS_WORK_CPUS=4 TEHAS_WORK_MEMORY=512m
t_run "$load && echo \"\$TEHAS_TICKET_CONCURRENCY \$TEHAS_CYCLE_CPUS \$TEHAS_CYCLE_MEMORY\""
t_eq "$t_rc" 0
t_eq "$t_out" "3 1.33 170m"

t_case "a memory budget in gigabytes is divided in MiB"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=2 TEHAS_WORK_CPUS=2 TEHAS_WORK_MEMORY=1g
t_run "$load && echo \"\$TEHAS_CYCLE_CPUS \$TEHAS_CYCLE_MEMORY\""
t_eq "$t_out" "1.00 512m"

t_case "the shares are exported, and one in the environment is not used"
T_ENV=(TEHAS_CYCLE_CPUS=64 TEHAS_CYCLE_MEMORY=64g)
t_run "$load && bash -c 'echo \$TEHAS_CYCLE_CPUS \$TEHAS_CYCLE_MEMORY'"
t_eq "$t_out" "1.00 512m"
T_ENV=()

t_case "a share below one CPU is refused, and both settings are named"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=3 TEHAS_WORK_CPUS=2 TEHAS_WORK_MEMORY=512m
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_TICKET_CONCURRENCY=3 leaves each cycle 0.66 CPUs of TEHAS_WORK_CPUS=2"

t_case "a memory share Docker would refuse is refused here"
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=2 TEHAS_WORK_CPUS=2 TEHAS_WORK_MEMORY=10m
t_run "$load; echo rc=\$?"
t_contains "$t_out" "rc=78"
t_contains "$t_out" "TEHAS_TICKET_CONCURRENCY=2 leaves each cycle 5m of TEHAS_WORK_MEMORY=10m"
rm -f "$T/bin/nproc"
```

- [ ] **Step 2: Write the failing test in `test/work_test.sh`**

Add this case just before `t_case "a checkout of another repository starts no container"`:

```bash
t_case "with two tickets side by side a cycle gets half the budget"
rm -f "$T"/calls/*
t_stub nproc 'echo 8'
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=2 TEHAS_WORK_CPUS=2 TEHAS_WORK_MEMORY=512m
t_run "$work 7"
t_eq "$(after --cpus)" "1.00"
t_eq "$(after --memory)" "256m"
rm -f "$T/bin/nproc"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_CPUS=1 TEHAS_WORK_MEMORY=512m PI_ENSEMBLE_HOST_ALIASES=models:192.0.2.9
```

- [ ] **Step 3: Run both and see them fail**

Run: `bash test/config_test.sh; bash test/work_test.sh`
Expected: FAIL lines for the new cases (`expected '1 1 512m', got '  '`, `expected '1.00', got '2'`), and the two `loading rejects TEHAS_TICKET_CONCURRENCY=...` cases.

- [ ] **Step 4: Implement in `lib/config.sh`**

In `_tehas_valid`, add the name to the counts:

```bash
    TEHAS_CONTROL_ISSUE | TEHAS_MAX_AWAITING | TEHAS_WORK_TIMEOUT | TEHAS_CONCURRENCY | TEHAS_TICKET_CONCURRENCY)
```

In `tehas_load`, change `local dir file v reason half` to `local dir file v reason half share`. After `export TEHAS_CONCURRENCY=${TEHAS_CONCURRENCY:-4}` add:

```bash
  # How many issues are worked on side by side, each in a cycle of its own.
  export TEHAS_TICKET_CONCURRENCY=${TEHAS_TICKET_CONCURRENCY:-1}
```

Add `TEHAS_TICKET_CONCURRENCY` to the first validation loop:

```bash
  for v in TEHAS_INDEXING TEHAS_ANALYTICS_COOKIE TEHAS_WORK_CPUS TEHAS_WORK_MEMORY TEHAS_CONCURRENCY \
    TEHAS_TICKET_CONCURRENCY TEHAS_HOST_ADDR TEHAS_MAX_AWAITING TEHAS_WORK_TIMEOUT; do
```

Directly after that loop add:

```bash
  # What one cycle's container gets: the budget divided between the tickets.
  # One ticket at a time gets the budget as written.
  if [ "$TEHAS_TICKET_CONCURRENCY" = 1 ]; then
    export TEHAS_CYCLE_CPUS=$TEHAS_WORK_CPUS TEHAS_CYCLE_MEMORY=$TEHAS_WORK_MEMORY
  else
    share=$(awk -v c="$TEHAS_WORK_CPUS" -v n="$TEHAS_TICKET_CONCURRENCY" 'BEGIN { printf "%.2f", int(c * 100 / n) / 100 }')
    if ! awk -v s="$share" 'BEGIN { exit !(s >= 1) }'; then
      tehas_err "TEHAS_TICKET_CONCURRENCY=$TEHAS_TICKET_CONCURRENCY leaves each cycle $share CPUs of TEHAS_WORK_CPUS=$TEHAS_WORK_CPUS; a cycle needs at least 1 (see $file)"
      return 78
    fi
    export TEHAS_CYCLE_CPUS=$share
    share=$(($(_tehas_mem_mb "$TEHAS_WORK_MEMORY") / TEHAS_TICKET_CONCURRENCY))
    if [ "$share" -lt 6 ]; then
      tehas_err "TEHAS_TICKET_CONCURRENCY=$TEHAS_TICKET_CONCURRENCY leaves each cycle ${share}m of TEHAS_WORK_MEMORY=$TEHAS_WORK_MEMORY; Docker needs at least 6m (see $file)"
      return 78
    fi
    export TEHAS_CYCLE_MEMORY=${share}m
  fi
```

- [ ] **Step 5: Implement in `libexec/work`**

In the cycle's `docker run` (the one with `--name "$name"`), replace the limits line:

```bash
  --cpus "$TEHAS_CYCLE_CPUS" --memory "$TEHAS_CYCLE_MEMORY" \
```

In the header, replace the four lines about `TEHAS_WORK_CPUS` and `TEHAS_WORK_MEMORY` with:

```bash
#   TEHAS_WORK_CPUS     docker --cpus for all cycles together: a time quota,
#                       not pinning [the host's CPUs minus one]
#   TEHAS_WORK_MEMORY   docker --memory for all cycles together, a hard limit
#                       [half the host's memory]
#   TEHAS_TICKET_CONCURRENCY  cycles side by side; this one gets the two
#                       limits above divided by it [1]
```

- [ ] **Step 6: Run the tests and see them pass**

Run: `bash test/config_test.sh && bash test/work_test.sh && bash test/index_test.sh && bash test/dev_test.sh`
Expected: each prints `<file>: N cases, 0 failed`.

- [ ] **Step 7: The manual and the example**

In `host/docs/tools.md`, replace the two rows for `TEHAS_WORK_CPUS` and `TEHAS_WORK_MEMORY` and add a row after `TEHAS_CONCURRENCY`:

```markdown
| `TEHAS_WORK_CPUS`               | the host's CPUs minus one, at least 1    | `docker --cpus` for all cycles together                         |
| `TEHAS_WORK_MEMORY`             | half the host's memory                   | `docker --memory` for all cycles together                       |
| `TEHAS_CONCURRENCY`             | `4`                                      | Workstreams side by side inside one cycle                       |
| `TEHAS_TICKET_CONCURRENCY`      | `1`                                      | Cycles side by side. Each cycle's container gets the two limits above divided by this; a share below one CPU is refused |
```

In `instances/example/instance.env`, after the `#TEHAS_CONCURRENCY=4` line and its blank line, add:

```bash
# Issues worked on side by side, each in a cycle and a container of its own.
# TEHAS_WORK_CPUS and TEHAS_WORK_MEMORY are divided between them, and every
# cycle runs TEHAS_CONCURRENCY workstreams, so this multiplies model calls too.
#TEHAS_TICKET_CONCURRENCY=1

```

Change the comment above `#TEHAS_WORK_CPUS=3` to begin `# Limits of the containers cycles run in, all of them together.` and keep the rest of that comment.

- [ ] **Step 8: Gate and commit**

Run: `scripts/verify.sh`
Expected: every line starts with `ok` (or `skip` for the console where cargo is missing).

```bash
git add lib/config.sh libexec/work test/config_test.sh test/work_test.sh host/docs/tools.md instances/example/instance.env
git commit -m "A ticket concurrency setting divides the cycle budget"
```

---

### Task 2: The poll, the cycle and its unit

Still one cycle at a time. After this task the dispatcher no longer waits for a cycle.

**Files:**
- Create: `lib/cycles.sh`, `libexec/cycle`, `host/systemd/tehas-cycle@.service`, `test/cycle_test.sh`
- Modify: `libexec/dispatch`, `libexec/index`, `lib/config.sh`, `bin/tehasbot`, `install.sh`, `host/systemd/tehas-dispatch.service`, `libexec/status`, `libexec/health`, `libexec/summary` (header comments only for the last three)
- Modify: `host/docs/tools.md`, `host/docs/operating.md`
- Test: `test/dispatch_test.sh`, `test/cycle_test.sh`, `test/index_test.sh`, `test/config_test.sh`, `test/cli_test.sh`, `test/install_test.sh`

**Interfaces:**
- Consumes: nothing from Task 1.
- Produces:
  - `lib/cycles.sh`: `tehas_cycle_running <issue>` returns 0 when that issue's cycle holds `$TEHAS_HOME/state/cycles/<issue>.lock`; `tehas_cycles_running` prints the issues with a running cycle, one per line. Both need `TEHAS_HOME` only.
  - `tehasbot cycle <issue>`: holds `state/cycles/<issue>.lock` while it runs, writes `state/cycles/<issue>.taken` once it holds it. Exit 0 reported, 3 already running, 64 bad argument, 78 bad settings.
  - Test hooks kept by `tehas_load`: `TEHAS_CYCLE_START_CMD` (called as `<cmd> <issue>` in place of `systemctl --user start tehas-cycle@<issue>.service`) and `TEHAS_CYCLE_START_WAIT` (seconds, default 30).
  - `libexec/dispatch` variables Task 6 edits: `running` (issues, space separated), `want` (how many to start), arrays `picked` and `by`.

- [ ] **Step 1: `lib/cycles.sh` and its test**

Add to `test/index_test.sh`, replacing the case `while a cycle holds the dispatch lock nothing is indexed` with these two:

```bash
t_case "while the dispatcher's lock is held nothing is indexed"
reset
mkdir -p "$T/home/tehas/state"
exec 9>"$T/home/tehas/state/dispatch.lock"
flock -n 9
t_run "$index"
t_eq "$t_rc" 3
t_contains "$t_out" "the dispatcher's lock is held"
t_eq "$(t_calls docker)" ""
exec 9>&-

t_case "while a cycle holds its lock nothing is indexed, and the issue is named"
reset
mkdir -p "$T/home/tehas/state/cycles"
exec 9>"$T/home/tehas/state/cycles/12.lock"
flock -n 9
t_run "$index"
t_eq "$t_rc" 3
t_contains "$t_out" "a cycle is running (issue 12)"
t_eq "$(t_calls docker)" ""
exec 9>&-

t_case "the lock file of a cycle that ended holds nothing back"
reset
t_run "$index"
t_eq "$t_rc" 0
rm -rf "$T/home/tehas/state/cycles"
```

Run: `bash test/index_test.sh`
Expected: FAIL on the first two new cases (the message, and exit 0 where 3 is expected).

Create `lib/cycles.sh`:

```bash
# shellcheck shell=bash
# Which cycles are running. Sourced by the dispatcher and by tehas index.
#
# tehasbot cycle holds $TEHAS_HOME/state/cycles/<issue>.lock for as long as it
# lives. A lock nobody holds is a cycle that ended, however it ended, so a
# cycle that died is never counted.
#
#   tehas_cycle_running <issue>   0 if that issue's cycle holds its lock
#   tehas_cycles_running          the issues with a running cycle, one per line
#
# Needs TEHAS_HOME. Reads only: it takes each lock for an instant and lets go.

tehas_cycle_running() {
  local lock=$TEHAS_HOME/state/cycles/$1.lock
  [ -e "$lock" ] && ! flock -n "$lock" true
}

tehas_cycles_running() {
  local lock n
  for lock in "$TEHAS_HOME"/state/cycles/*.lock; do
    [ -e "$lock" ] || continue
    n=${lock##*/}
    n=${n%.lock}
    if tehas_cycle_running "$n"; then printf '%s\n' "$n"; fi
  done
  return 0
}
```

In `libexec/index`, source it after `lib/config.sh`:

```bash
# shellcheck source=lib/cycles.sh
. "$here/../lib/cycles.sh"
```

Replace the comment and the `if [ "$cycle" = 0 ]; then ... elif` head (keep the `elif` branch body as it is) with:

```bash
# The index is written only under the dispatcher's lock and while no cycle is
# running: a poll holds that lock while it starts cycles, and a cycle reads
# the index for as long as it lives. Called by the dispatcher, the lock is
# held by the caller, who has also looked at the cycles.
mkdir -p "$TEHAS_HOME/state" "$TEHAS_HOME/logs" "$TEHAS_HOME/tmp"
if [ "$cycle" = 0 ]; then
  exec 8>"$TEHAS_HOME/state/dispatch.lock"
  flock -n 8 || { echo "tehas index: the dispatcher's lock is held by a poll or another index run, nothing was indexed; run it again in a moment" >&2; exit 3; }
  running=$(tehas_cycles_running | tr '\n' ' ')
  [ -z "$running" ] ||
    { echo "tehas index: a cycle is running (issue ${running% }), nothing was indexed; run it again afterwards" >&2; exit 3; }
elif [ "$(jq -r '.commit // empty' "$marker" 2>/dev/null)" = "$base" ]; then
```

In the header of `libexec/index`, replace the line `#              3 a cycle is running; nothing was done.` with:

```bash
#              3 a cycle is running, or a poll or another index run holds the
#                dispatcher's lock; nothing was done.
```

and in the usage, replace `holds the dispatcher's lock, and nothing` with `holds the dispatcher's lock and has seen that no cycle runs, and nothing`.

Run: `bash test/index_test.sh`
Expected: `index_test.sh: N cases, 0 failed`.

- [ ] **Step 2: The test hooks in `lib/config.sh`**

In `test/config_test.sh`, replace the body of the case `the test hooks survive from the environment` with:

```bash
t_instance "${T_MINIMAL[@]}"
T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index
  TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2)
t_run "$load && echo \"\$TEHAS_WORK_CMD \$TEHAS_STATUS_CMD \$TEHAS_INDEX_CMD \$TEHAS_CYCLE_START_CMD \$TEHAS_CYCLE_START_WAIT\""
t_eq "$t_out" "fake-work fake-status fake-index fake-start 2"
T_ENV=()
```

Run: `bash test/config_test.sh` and expect that case to fail.

In `lib/config.sh`, extend the kept names in `tehas_load`:

```bash
      TEHAS_HOME | TEHAS_INSTANCE_DIR | TEHAS_WORK_CMD | TEHAS_STATUS_CMD | TEHAS_INDEX_CMD | \
        TEHAS_CYCLE_START_CMD | TEHAS_CYCLE_START_WAIT) ;;
```

and the header's rule to read:

```bash
#   - instance.env is authoritative. Settings of the same name in the
#     environment are dropped; only TEHAS_HOME, TEHAS_INSTANCE_DIR and the test
#     hooks TEHAS_WORK_CMD, TEHAS_STATUS_CMD, TEHAS_INDEX_CMD,
#     TEHAS_CYCLE_START_CMD and TEHAS_CYCLE_START_WAIT are kept.
```

Run: `bash test/config_test.sh`
Expected: `0 failed`.

- [ ] **Step 3: Write `test/cycle_test.sh`**

```bash
#!/usr/bin/env bash
# Tests for libexec/cycle against a stub gh: one cycle for one issue, and what
# is reported about it.
# shellcheck disable=SC2016  # snippets and stub bodies are expanded later
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

cycle="$T_ROOT/libexec/cycle"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget
workdir=$T/home/tehas/work/widget
state=$T/home/tehas/state/cycles

t_stub gh '
case "$1 $2" in
  "pr list") cat "$T/fx/pr.txt" 2>/dev/null ;;
esac'
# What the runner does is a file a case writes; without one it ends at once.
t_stub fake-work '[ ! -e "$T/fx/work.sh" ] || . "$T/fx/work.sh"'
t_stub fake-status

reset() {
  rm -rf "$T"/calls/* "$fx"/* "$state" "$workdir" "$T/home/tehas/logs"
  T_ENV=(TEHAS_WORK_CMD=fake-work TEHAS_STATUS_CMD=fake-status)
  t_instance "${T_MINIMAL[@]}"
}
# The state file a cycle leaves in its workspace.
ended() { # status step [branch]
  mkdir -p "$workdir/.pi/work-state"
  printf '{"pipelineState":{"status":"%s","currentStep":"%s","branchName":"%s"}}\n' "$1" "$2" "${3:-}" \
    >"$workdir/.pi/work-state/7.json"
}

for bad in "" "abc" "07" "0" "7 8" "7x" "-7"; do
  t_case "'$bad' is not an issue: nothing runs and nothing is written"
  reset
  t_run "$cycle $bad"
  t_eq "$t_rc" 64
  t_contains "$t_out" "usage: tehasbot cycle <issue>"
  t_eq "$(t_calls gh)$(t_calls fake-work)" ""
  t_eq "$(find "$state" 2>/dev/null | wc -l)" 0
done

t_case "an invalid instance runs nothing, exit 78"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$cycle 7"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)$(t_calls fake-work)" ""

t_case "a first cycle starts plainly, a later one for the same issue restarts"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
rm -f "$T"/calls/*
t_run "$cycle 7"
t_eq "$(t_calls fake-work)" "7 --restart"

t_case "it says that it holds its lock, and lets go of it at the end"
t_eq "$(find "$state/7.taken" | wc -l)" 1
t_eq "$(flock -n "$state/7.lock" echo free)" "free"

t_case "the runner's output is in the issue's log"
reset
echo 'echo "from the runner"' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$(cat "$T/home/tehas/logs/dispatch-work-7.log")" "from the runner"

t_case "a cycle that ends without a pull request is handed to a human"
reset
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "Tehas cycle stopped without a pull request"
t_contains "$t_out" "done #7 rc=0 status=unknown"
t_eq "$(t_calls fake-status | wc -l)" 1

t_case "the runner's exit status is in the report"
reset
echo 'exit 3' >"$fx/work.sh"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "(runner exit 3)"

t_case "a cycle that ends with a pull request waits for a human and is reported on it"
reset
ended handoff commit-pr feature/x
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$t_rc" 0
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label tehas:awaiting-merge"
t_contains "$(t_calls gh)" "pr comment https://github.com/acme/widget/pull/9"
t_contains "$(t_calls gh)" "pr list --repo acme/widget --head feature/x --author @me"
t_contains "$(t_calls gh)" "No token usage was recorded for this cycle."

t_case "the token usage per role is posted on the pull request"
reset
mkdir -p "$workdir/.pi/work-state"
echo '{"pipelineState":{"status":"handoff","currentStep":"commit-pr","branchName":"feature/x"},
  "eventLog":[{"role":"developer","usage":{"turns":2,"input":10,"output":5,"cacheRead":1}},
              {"role":"developer","usage":{"turns":1,"input":20,"output":5,"cacheRead":0}}]}' >"$workdir/.pi/work-state/7.json"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "| developer | 2 | 3 | 30 | 10 | 1 |"

t_case "a merged cycle only loses its running label"
reset
ended merged done feature/x
t_run "$cycle 7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running"
t_lacks "$(t_calls gh)" "--add-label"
t_contains "$(t_calls gh)" "The change was merged."

t_case "a state file in the product checkout is not the cycle's: the container never writes there"
reset
mkdir -p "$checkout/.pi/work-state"
echo '{"pipelineState":{"status":"merged","currentStep":"done","branchName":"feature/x"}}' >"$checkout/.pi/work-state/7.json"
t_run "$cycle 7"
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
rm -rf "$checkout"

t_case "a second cycle for the same issue runs nothing, exit 3"
reset
mkdir -p "$state"
exec 9>"$state/7.lock"
flock -n 9
t_run "$cycle 7"
t_eq "$t_rc" 3
t_contains "$t_out" "a cycle for #7 is already running"
t_eq "$(t_calls gh)$(t_calls fake-work)" ""
t_eq "$(find "$state/7.taken" 2>/dev/null | wc -l)" 0
exec 9>&-

t_case "a cycle for another issue is not held back"
reset
mkdir -p "$state"
exec 9>"$state/8.lock"
flock -n 9
t_run "$cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
exec 9>&-

t_case "a stop is passed on to the runner, and the cycle is still reported"
reset
# The runner says when its trap is set, so the signal cannot arrive before it.
echo 'sleep 30 & s=$!
trap "kill \$s; : >\"\$T/calls/work.term\"; exit 2" TERM
: >"$T/calls/work.ready"
wait "$s"' >"$fx/work.sh"
t_run "$cycle 7 & pid=\$!
until [ -e \"\$T/calls/work.ready\" ]; do sleep 0.1; done
kill -TERM \$pid; wait \$pid; echo rc=\$?"
t_contains "$t_out" "rc=0"
t_eq "$(find "$T/calls/work.term" | wc -l)" 1
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "(runner exit 2)"

# Outside the tests nothing names the two scripts: the cycle runs the ones in
# its own directory, whatever PATH holds.
t_case "without the test seams it runs the scripts beside it, not names on PATH"
reset
T_ENV=()
tree=$T/tree
mkdir -p "$tree/libexec" "$tree/lib"
cp "$cycle" "$tree/libexec/cycle"
cp "$T_ROOT/lib/config.sh" "$tree/lib/config.sh"
for name in work status; do cp "$T/bin/fake-$name" "$tree/libexec/$name"; done
for name in tehas tehasbot tehas-work tehas-status; do t_stub "$name"; done
t_run "$tree/libexec/cycle 7"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-work)" "7"
t_eq "$(t_calls fake-status | wc -l)" 1
for name in tehas tehasbot tehas-work tehas-status; do t_eq "$(t_calls "$name")" ""; done

t_done
```

Run: `chmod +x test/cycle_test.sh; bash test/cycle_test.sh`
Expected: FAIL throughout, `libexec/cycle: No such file or directory`.

- [ ] **Step 4: Write `libexec/cycle`**

```bash
#!/usr/bin/env bash
# One tehas cycle for one issue, and its report.
#
# Deterministic, no model. It runs tehas work for the issue and reports the
# outcome on the issue (and the token usage on the pull request). The
# dispatcher starts it as tehas-cycle@<issue>.service, after it has checked who
# labelled the issue and marked it tehas:running. This command checks no
# authorisation, as tehas work checks none: run by hand it runs a cycle for
# any issue.
#
# Usage:
#   tehasbot cycle <issue>
#
# Exit: 0 the cycle ran and was reported, whatever its outcome; 3 a cycle for
# this issue is already running; 64 the argument is not an issue number; 78
# instance settings are missing or malformed.
#
# It holds state/cycles/<issue>.lock for as long as it runs. The dispatcher
# counts the held locks, so a cycle that dies frees its place. Once it holds
# the lock it writes state/cycles/<issue>.taken, which is what the dispatcher
# waits for.
#
# A TERM or INT is passed on to tehas work, which stops the container. The
# cycle is then reported like any other that stopped without a pull request.
#
# Instance settings (lib/config.sh): TEHAS_REPO.
# From the environment, for tests: TEHAS_WORK_CMD, TEHAS_STATUS_CMD.
set -uo pipefail

issue=${1:-}
if [ "$#" != 1 ] || ! [[ $issue =~ ^[1-9][0-9]*$ ]]; then
  echo "usage: tehasbot cycle <issue>" >&2
  exit 64
fi

here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lib/config.sh
. "$here/../lib/config.sh"
tehas_load || exit 78
tehas_require TEHAS_REPO

log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*"; }

repo=$TEHAS_REPO
workdir=$TEHAS_WORK_DIR
work=${TEHAS_WORK_CMD:-$here/work}
status_cmd=${TEHAS_STATUS_CMD:-$here/status}
cycles=$TEHAS_HOME/state/cycles

mkdir -p "$cycles" "$TEHAS_HOME/logs"
# The runner and its container inherit this lock. If this process is killed
# outright the place stays taken until they are gone too.
exec 9>"$cycles/$issue.lock"
flock -w 5 9 || { log "a cycle for #$issue is already running"; exit 3; }
: >"$cycles/$issue.taken"

comment() { gh issue comment "$issue" --repo "$repo" --body "$1" >/dev/null 2>&1 || log "WARN: comment on #$issue failed"; }
relabel() { gh issue edit "$issue" --repo "$repo" "$@" >/dev/null 2>&1 || log "WARN: label change on #$issue failed: $*"; }

started=$(date +%s)
# A relabelled issue gets a fresh cycle, not whatever the last one parked.
# The workspace is new every time, so the record of an earlier cycle is kept
# here, on the host.
restart=""
[ -e "$cycles/$issue" ] && restart=--restart
: >"$cycles/$issue"
"$work" "$issue" ${restart:+"$restart"} >>"$TEHAS_HOME/logs/dispatch-work-$issue.log" 2>&1 &
wpid=$!
stopped=0
trap 'stopped=1; kill -TERM "$wpid" 2>/dev/null' TERM INT
wait "$wpid"
rc=$?
# A signal ends the wait early, with a status that is not the runner's. The
# runner is still stopping its container: wait again for its own status.
while [ "$stopped" = 1 ]; do
  stopped=0
  wait "$wpid"
  rc=$?
done
trap - TERM INT
mins=$((($(date +%s) - started) / 60))

# The one thing read back from the tree the container wrote, and only as data.
state=$workdir/.pi/work-state/$issue.json
status=$(jq -r '.pipelineState.status // "unknown"' "$state" 2>/dev/null || echo unknown)
step=$(jq -r '.pipelineState.currentStep // "-"' "$state" 2>/dev/null || echo -)
branch=$(jq -r '.pipelineState.branchName // ""' "$state" 2>/dev/null || echo "")
usage=$(jq -r '
  [.eventLog[] | select(.usage) | {role: (.role // "driver"), u: .usage}] as $e
  | if ($e | length) == 0 then "" else
      (["| Role | Dispatches | Turns | Input | Output | Cache read |", "|---|---|---|---|---|---|"]
       + ($e | group_by(.role) | map("| \(.[0].role) | \(length) | \(map(.u.turns) | add) | \(map(.u.input) | add) | \(map(.u.output) | add) | \(map(.u.cacheRead) | add) |"))
       + ["| **total** | \($e | length) | \($e | map(.u.turns) | add) | \($e | map(.u.input) | add) | \($e | map(.u.output) | add) | \($e | map(.u.cacheRead) | add) |"])
      | join("\n") end' "$state" 2>/dev/null || echo "")

pr=""
if [ -n "$branch" ]; then
  # The branch name comes from a file the cycle wrote. Only a pull request the
  # bot itself opened counts, so it cannot point at somebody else's.
  pr=$(gh pr list --repo "$repo" --head "$branch" --author "@me" --state open --json url --jq '.[0].url // ""' 2>/dev/null || echo "")
fi

summary="status \`$status\`, last step \`$step\`, $mins min"
if [ "$rc" = 0 ] && [ "$status" = merged ]; then
  relabel --remove-label tehas:running
  comment "Tehas cycle finished: $summary. The change was merged."
elif [ "$rc" = 0 ] && [ -n "$pr" ]; then
  relabel --remove-label tehas:running --add-label tehas:awaiting-merge
  comment "Tehas cycle finished: $summary. Pull request waiting for a human: $pr"
  gh pr comment "$pr" --body "Tehas cycle for #$issue: $summary.

${usage:-No token usage was recorded for this cycle.}" >/dev/null 2>&1 || log "WARN: comment on $pr failed"
else
  relabel --remove-label tehas:running --add-label needs-human-attention
  comment "Tehas cycle stopped without a pull request: $summary (runner exit $rc). Read the handoff comment if there is one; the run log is \`~/tehas/logs/dispatch-work-$issue.log\` on \`$(hostname)\`. To retry, remove \`needs-human-attention\` and apply \`tehas:ready\` again.

${usage}"
fi
log "done #$issue rc=$rc status=$status step=$step minutes=$mins pr=${pr:-none}"
"$status_cmd"
exit 0
```

Run: `chmod +x libexec/cycle && bash test/cycle_test.sh`
Expected: `cycle_test.sh: N cases, 0 failed`. The lock case takes five seconds.

- [ ] **Step 5: `bin/tehasbot` and the unit**

In `test/cli_test.sh`:
- add `cycle` to the list `for name in dispatch work dev index update labels summary status health energy protolab; do` (after `dispatch`),
- in the case `tehasbot runs the script of each command with the remaining arguments`, change the run to `"$bot dispatch && $bot cycle 7 && $bot status && $bot health && $bot energy --every && $bot protolab"` and add the line `cycle|7` after `dispatch` in the expected text,
- in the case `every command in the usage of tehasbot has an executable script`, change the expected names to `"dispatch cycle status health energy protolab "`.

Run: `bash test/cli_test.sh` and expect those cases to fail.

In `bin/tehasbot`, add to the usage after the `dispatch` line:

```bash
#   tehasbot cycle <issue>        one cycle and its report (tehas-cycle@<issue>.service)
```

and to the case: `dispatch | cycle | status | health | energy | protolab)`.

Create `host/systemd/tehas-cycle@.service`:

```ini
[Unit]
Description=Tehas: the work cycle for issue %i

[Service]
Type=exec
ExecStart=/bin/bash -c ". %h/tehas/env.sh && exec %h/tehas/bin/tehasbot cycle %i >> %h/tehas/logs/dispatch.log 2>&1"
# A stop reaches the command only. It passes it on to the cycle's container
# and then reports on the issue, which takes a few calls to GitHub.
KillMode=mixed
TimeoutStopSec=120
```

Replace `host/systemd/tehas-dispatch.service` with:

```ini
[Unit]
Description=Tehas: one poll of the issue queue, starts the cycles

[Service]
Type=oneshot
ExecStart=/bin/bash -c ". %h/tehas/env.sh && exec %h/tehas/bin/tehasbot dispatch >> %h/tehas/logs/dispatch.log 2>&1"
# A poll that refreshes the code index can take minutes.
TimeoutStartSec=infinity
```

Run: `bash test/cli_test.sh`
Expected: `0 failed`.

- [ ] **Step 6: `install.sh`**

In `test/install_test.sh`, in the `files=` text add, each in its sorted place: `./.config/systemd/user/tehas-cycle@.service` as the first line, `./tehas/lib/cycles.sh` after `./tehas/lib/config.sh`, and `./tehas/libexec/cycle` after `./tehas/libexec/console.stamp`. In `next_steps`, add `systemd/tehas-cycle@.service` before `systemd/tehas-dispatch.service`.

Run: `bash test/install_test.sh` and expect `a fresh install: exactly these files` to fail.

In `install.sh`, add to `files=(` after the `lib/config.sh` line:

```bash
  "lib/cycles.sh|$TEHAS_HOME/lib/cycles.sh|644"
```

before the `tehas-dispatch.service` line:

```bash
  "systemd/tehas-cycle@.service|$units/tehas-cycle@.service|644"
```

and add `cycle` to the libexec loop: `for f in cycle dev dispatch energy health index labels protolab status summary update work; do`.

Run: `bash test/install_test.sh`
Expected: `0 failed`.

- [ ] **Step 7: Rewrite the dispatcher's tests**

In `test/dispatch_test.sh`:

(a) Delete the line `workdir=$T/home/tehas/work/widget`.

(b) Replace the three stubs `fake-work`, `fake-status`, `fake-index` with:

```bash
# Stands in for systemctl --user start tehas-cycle@<issue>.service. A real
# cycle says it holds its lock by writing the .taken file.
t_stub fake-start '[ ! -e "$T/fx/start-fails" ] || exit 1
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/start.fd8"
[ -e "$T/fx/start-silent" ] || : >"$TEHAS_HOME/state/cycles/$1.taken"'
t_stub fake-status
# Notes what the checkout was at and whether a cycle had already started.
t_stub fake-index 'git -C "$T/home/tehas/repos/widget" rev-parse HEAD >"$T/calls/index.head"
[ ! -e "$T/calls/fake-start" ] || : >"$T/calls/index.late"
[ ! -e "$T/fx/index-fails" ] || exit 1'
```

(c) In `reset()`, replace the first line and the `T_ENV` line:

```bash
  rm -f "$T"/calls/* "$fx"/edit-* "$fx"/start-*
```

```bash
  T_ENV=(TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index)
```

and add after `reset()`:

```bash
# A cycle that is running: its lock, held by this shell until release.
held=()
hold() { # issue
  local fd
  mkdir -p "$T/home/tehas/state/cycles"
  exec {fd}>"$T/home/tehas/state/cycles/$1.lock"
  flock -n "$fd"
  held+=("$fd")
}
release() {
  local fd
  for fd in "${held[@]}"; do exec {fd}>&-; done
  held=()
}
```

(d) Run `sed -i 's/fake-work/fake-start/g' test/dispatch_test.sh`.

(e) Delete these four cases whole. They are in `test/cycle_test.sh` now: `a cycle that ends without a pull request is handed to a human`, `a cycle that ends with a pull request waits for a human and is reported on it`, `a state file in the product checkout is not the cycle's: the container never writes there`, `a first cycle starts plainly, a later one for the same issue restarts`.

(f) Add these cases just before the comment `# Arguments. A poll starts only from an argument this command knows`:

```bash
t_case "the cycle is started for the issue and told the labeller in a comment"
reset
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "Tehas cycle started on"
t_contains "$(t_calls gh)" "authorized by @alice"

t_case "the poll does not wait for the cycle: its lock is free afterwards"
t_eq "$(flock -n "$T/home/tehas/state/dispatch.lock" echo free)" "free"

t_case "the poll's lock is not passed on to the cycle it starts"
t_eq "$(find "$T/calls/start.fd8" 2>/dev/null | wc -l)" 0

t_case "the status is refreshed before the poll and after the start"
t_eq "$(t_calls fake-status | wc -l)" 2

t_case "while a cycle is running nothing is started"
reset
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: a cycle is running on #9"
release

t_case "the lock file of a cycle that ended holds nothing back"
reset
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/9.lock"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "a cycle that cannot be started is handed to a human, exit 1"
reset
: >"$fx/start-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_contains "$(t_calls gh)" "could not start the cycle"

t_case "a cycle that never says it holds its lock is handed to a human"
reset
: >"$fx/start-silent"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "--remove-label tehas:running --add-label needs-human-attention"

t_case "a start marker left by an earlier cycle does not count"
reset
: >"$fx/start-silent"
mkdir -p "$T/home/tehas/state/cycles"
: >"$T/home/tehas/state/cycles/7.taken"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: the cycle for #7 did not start"

t_case "a cycle that ended at once still counts as started"
reset
# fake-start writes the marker and is gone: no lock is held when the poll looks.
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$(t_calls gh)" "needs-human-attention"
t_lacks "$t_out" "did not start"
```

(g) Replace the last case (`without the test seams it runs the scripts beside it, not names on PATH`) and its comment with:

```bash
# Outside the tests nothing names the scripts: the dispatcher runs the ones in
# its own directory, whatever PATH holds, and starts a cycle through systemd.
t_case "without the test seams it runs the scripts beside it and starts the unit"
reset
T_ENV=()
tree=$T/tree
mkdir -p "$tree/libexec" "$tree/lib"
cp "$dispatch" "$tree/libexec/dispatch"
cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/cycles.sh" "$tree/lib/"
for name in status index; do cp "$T/bin/fake-$name" "$tree/libexec/$name"; done
t_stub systemctl 'n=${3#tehas-cycle@}
: >"$TEHAS_HOME/state/cycles/${n%.service}.taken"'
for name in tehas tehasbot tehas-work tehas-status tehas-index; do t_stub "$name"; done
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl)" "--user start tehas-cycle@7.service"
t_contains "$(t_calls fake-index)" "--before-cycle"
t_eq "$(t_calls fake-status | wc -l)" 2
for name in tehas tehasbot tehas-work tehas-status tehas-index; do t_eq "$(t_calls "$name")" ""; done
```

Run: `bash test/dispatch_test.sh`
Expected: many FAIL lines: `expected '7', got ''` for `fake-start`, and the new cases.

- [ ] **Step 8: Rewrite `libexec/dispatch`**

Replace the header, lines 2 to 34, with:

```bash
# The tehas dispatcher: one poll of the GitHub issue queue.
#
# Deterministic, no model. It picks at most one open issue labelled
# tehas:ready, marks it tehas:running and starts its cycle as
# tehas-cycle@<issue>.service. It does not wait for the cycle: tehasbot cycle
# runs it and reports the outcome. A systemd timer runs a poll every two
# minutes, whatever the cycles are doing.
#
# Usage:
#   tehas dispatch               one poll
#   tehas dispatch --dry-run     say what a poll would do, change nothing
#   tehas dispatch -h|--help     print this text and exit 0
#
# Exit: 0 the poll ran, or there was nothing to do; 64 an argument this command
# does not know, which starts no poll; 78 instance settings are missing or
# malformed; 1 a call to GitHub failed or its answer could not be read, or a
# cycle could not be started.
#
# Rules, in order:
#   1. Label tehas:pause on the control issue stops all dispatch.
#   2. No new cycle while one is running.
#   3. No new cycle while TEHAS_MAX_AWAITING tehas PRs wait for a human.
#   4. An issue is eligible only if its tehas:ready label was applied by an
#      account in TEHAS_ALLOWED_LABELERS. The bot is never in that list.
#   5. tehas:urgent first, then the lowest issue number.
#
# Instance settings (lib/config.sh; defaults in brackets):
#   TEHAS_REPO              owner/name of the product repository, required
#   TEHAS_CONTROL_ISSUE     issue that carries tehas:pause, required
#   TEHAS_ALLOWED_LABELERS  space-separated logins, required
#   TEHAS_MAX_AWAITING      open tehas:awaiting-merge issues allowed [3]
#   TEHAS_WORK_REPO         checkout cycles are cloned from [$TEHAS_HOME/repos/<name>]
#   TEHAS_BRANCH            branch work lands on [the checkout's origin/HEAD]
# From the environment, for tests: TEHAS_STATUS_CMD, TEHAS_INDEX_CMD,
# TEHAS_CYCLE_START_CMD (in place of systemctl, called with the issue) and
# TEHAS_CYCLE_START_WAIT (seconds to wait for a started cycle [30]).
```

Leave the argument handling as it is. Replace everything from the line `here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)` down to and including the `flock -n 8 ...` line with:

```bash
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lib/config.sh
. "$here/../lib/config.sh"
# shellcheck source=lib/cycles.sh
. "$here/../lib/cycles.sh"
tehas_load || exit 78

log() { printf '%s %s\n' "$(date -u +%FT%TZ)" "$*"; }

tehas_require_instance_settings
repo=$TEHAS_REPO
control=$TEHAS_CONTROL_ISSUE
allowed=" $TEHAS_ALLOWED_LABELERS "
max_awaiting=$TEHAS_MAX_AWAITING
checkout=$TEHAS_WORK_REPO
status_cmd=${TEHAS_STATUS_CMD:-$here/status}
index_cmd=${TEHAS_INDEX_CMD:-$here/index}
start_wait=${TEHAS_CYCLE_START_WAIT:-30}
# Known before anything is touched: a cycle must never start from a guess,
# nor in a checkout of another repository.
tehas_checkout || exit 78
branch=$(tehas_branch) || exit 78

mkdir -p "$TEHAS_HOME/state/cycles" "$TEHAS_HOME/logs"
# Held for the poll only. A cycle holds the lock of its own issue.
exec 8>"$TEHAS_HOME/state/dispatch.lock"
flock -n 8 || { [ "$dry" = 1 ] && log "another poll, or tehas index, holds the dispatcher's lock"; exit 0; }
```

Leave `gh_json`, the status call, the repository's name check and the pause switch as they are. Directly after the pause switch (before the comment `# 2. Do not pile up unreviewed work.`) insert:

```bash
# 2. One cycle at a time. A cycle that died holds no lock and is not counted.
running=$(tehas_cycles_running | tr '\n' ' ')
running=${running% }
if [ -n "$running" ]; then
  [ "$dry" = 1 ] && log "holding: a cycle is running on #$running"
  exit 0
fi
want=1
```

Renumber the two comments below it to `# 3. Do not pile up unreviewed work.` and `# 4. and 5. Candidates: urgent first, then by number.`

Replace the candidate loop's head and tail. The head `issue=""` and `for n in $candidates; do` becomes:

```bash
picked=()
declare -A by=()
for n in $candidates; do
  [ "${#picked[@]}" -lt "$want" ] || break
```

and the loop's last two lines `issue=$n` and `break` become:

```bash
  picked+=("$n")
  by[$n]=$who
```

Replace everything after the loop's `done` to the end of the file with:

```bash
if [ "${#picked[@]}" = 0 ]; then
  [ "$dry" = 1 ] && log "nothing to do"
  exit 0
fi
if [ "$dry" = 1 ]; then
  for n in "${picked[@]}"; do log "would run a cycle for #$n (labelled by ${by[$n]})"; done
  exit 0
fi

comment() { gh issue comment "$1" --repo "$repo" --body "$2" >/dev/null 2>&1 || log "WARN: comment on #$1 failed"; }

# start_cycle <issue>: 0 once the cycle says it holds its lock, by writing
# the .taken file. Looking at the lock would miss a cycle that ended within
# the wait. The poll's own lock is not passed on: a cycle outlives the poll.
start_cycle() {
  local n=$1 taken=$TEHAS_HOME/state/cycles/$1.taken i
  rm -f "$taken"
  if [ -n "${TEHAS_CYCLE_START_CMD:-}" ]; then
    "$TEHAS_CYCLE_START_CMD" "$n" 8>&- || return 1
  else
    systemctl --user start "tehas-cycle@$n.service" 8>&- || return 1
  fi
  for ((i = 0; i < start_wait * 2; i++)); do
    [ -e "$taken" ] && return 0
    sleep 0.5
  done
  return 1
}

# Cycles start from the product's default branch; keep the checkout current.
# tehas work clones the cycle's workspace from it. No container is ever given
# this checkout, so running git here runs nothing an agent wrote. Put it back
# on the branch only when it is clean: never discard someone's changes.
if [ -z "$(git -C "$checkout" status --porcelain --untracked-files=no)" ]; then
  git -C "$checkout" checkout -q "$branch" || log "WARN: could not switch $checkout to $branch"
else
  log "WARN: $checkout has uncommitted changes, leaving its branch alone"
fi
if ! git -C "$checkout" fetch -q origin || ! git -C "$checkout" merge -q --ff-only "origin/$branch"; then
  log "WARN: could not fast-forward $checkout"
fi

# The agents search an index of the tree the cycle is about to be cloned from.
# A cycle without a fresh index still works, so a failure here is a warning.
index_log=$TEHAS_HOME/logs/dispatch-work-${picked[0]}.log
"$index_cmd" --before-cycle >>"$index_log" 2>&1 ||
  log "WARN: the code index was not refreshed, see $index_log"

failed=0
for n in "${picked[@]}"; do
  who=${by[$n]}
  log "start #$n (labelled by $who)"
  # The cycle starts only once the issue is marked as taken. If the label
  # cannot be changed, tehas:ready would stay and every later poll would start
  # the same issue again.
  if ! gh issue edit "$n" --repo "$repo" --add-label tehas:running --remove-label tehas:ready >/dev/null 2>&1; then
    log "ERROR: could not mark #$n as running, not starting a cycle"
    exit 1
  fi
  comment "$n" "Tehas cycle started on \`$(hostname)\` at $(date -u +'%F %H:%M') UTC (authorized by @$who's \`tehas:ready\` label). This issue is owned by the cycle until \`tehas:running\` is removed."
  if ! start_cycle "$n"; then
    log "ERROR: the cycle for #$n did not start"
    gh issue edit "$n" --repo "$repo" --remove-label tehas:running --add-label needs-human-attention >/dev/null 2>&1 ||
      log "WARN: label change on #$n failed"
    comment "$n" "Tehas could not start the cycle for this issue on \`$(hostname)\`: \`tehas-cycle@$n.service\` did not come up. See \`journalctl --user -u tehas-cycle@$n\` and \`~/tehas/logs/dispatch.log\` there. To retry, remove \`needs-human-attention\` and apply \`tehas:ready\` again."
    failed=1
  fi
done
"$status_cmd"
exit "$failed"
```

Run: `bash test/dispatch_test.sh`
Expected: `dispatch_test.sh: N cases, 0 failed`.

- [ ] **Step 9: Comments that say "every two minutes during a cycle"**

The poll now runs every two minutes whatever a cycle does. Reword these header comments, changing nothing else:

- `libexec/status`: `The dispatcher runs it at every poll and every two minutes while a cycle is running.` becomes `The dispatcher runs it at every poll, and a cycle when it ends.`
- `libexec/health`: `refreshed at every poll and every two minutes during a cycle.` becomes `refreshed at every poll and at the end of every cycle.`
- `libexec/summary`: the same phrase becomes `at every poll and at the end of every cycle`. Keep the rest of the sentence.

- [ ] **Step 10: The manual**

`host/docs/tools.md`:
- In `## tehas dispatch`, replace `tehas dispatch                 # run at most one cycle, then return` with `tehas dispatch                 # start at most one cycle, then return`.
- Replace the paragraph that begins `Before a cycle it puts the product checkout` with:

```markdown
When it has an issue to start, it puts the product checkout on the default
branch and fast-forwards it, then refreshes the code index with
[tehas index](#tehas-index) when the branch has moved since the last indexing.
An index that cannot be refreshed is a warning in the log; the cycle still
runs. It then marks the issue `tehas:running` and starts
`tehas-cycle@<issue>.service`, which runs [tehasbot cycle](#tehasbot-cycle).
It does not wait for the cycle. If the unit does not come up within 30
seconds, the issue gets `needs-human-attention` and the poll ends with exit 1.
The dispatcher runs the index and status scripts from its own directory,
`~/tehas/libexec`, not by a name on `PATH`.
```

- In `## tehas index`, add after the first paragraph that follows the code block:

```markdown
It refuses, with exit 3, while a cycle is running or while a poll or another
index run holds the dispatcher's lock. The index is only ever written when
nothing is reading it.
```

- In `## tehasbot`, add to the code block after the `dispatch` line `tehasbot cycle <issue>        # one cycle and its report; what tehas-cycle@<issue>.service runs`, and replace the sentence after the block with:

```markdown
The units are `tehas-dispatch.timer`, `tehas-energy.service`,
`tehas-protolab.timer` and one `tehas-cycle@<issue>.service` per running
cycle. A cycle's container is `tehas-work-<issue>`.
```

- Add a section before `## tehasbot status and the console`:

````markdown
## tehasbot cycle

One cycle for one issue, and its report. The dispatcher starts it as a unit;
it is not typed in daily use.

```
systemctl --user status 'tehas-cycle@*'    # the cycles that are running
systemctl --user stop tehas-cycle@12       # end the cycle for issue 12
```

It runs [tehas work](#tehas-work) for the issue, reads the cycle's state file
from the workspace, finds the pull request the bot opened, and relabels and
comments: merged, `tehas:awaiting-merge`, or `needs-human-attention`. A
stopped unit stops the container and is reported as a cycle that ended without
a pull request. It checks no authorisation: the dispatcher did that before it
started the unit.

| Exit | Meaning                                              |
| ---- | ---------------------------------------------------- |
| 0    | The cycle ran and was reported, whatever its outcome |
| 3    | A cycle for this issue is already running            |
| 64   | The argument is not an issue number                  |
| 78   | The instance settings are invalid                    |
````

  If `## tehas work` has another anchor in this file, link to that one.
- In `## tehasbot status and the console`, replace `the dispatcher runs it at every poll and every two minutes during a cycle` with `the dispatcher runs it at every poll, and a cycle when it ends`. In `## tehasbot health`, replace `refreshed at every poll and every two minutes during a cycle` with `refreshed at every poll and at the end of every cycle`.

`host/docs/operating.md`: replace the line `tehas dispatch               # do it (returns when the cycle ends)` with `tehas dispatch               # do it (returns once the cycle is started)`.

`host/docs/troubleshooting.md`: replace the row `| Is a cycle running? | ...` with:

```markdown
| Is a cycle running?              | `systemctl --user status 'tehas-cycle@*'`, or `docker ps --filter name=tehas-work` |
```

Keep the column widths of the neighbouring rows.

- [ ] **Step 11: Gate and commit**

Run: `scripts/verify.sh`
Expected: every line `ok` (or `skip`).

```bash
git add lib/cycles.sh lib/config.sh libexec/cycle libexec/dispatch libexec/index libexec/status libexec/health libexec/summary \
  bin/tehasbot install.sh host/systemd/tehas-cycle@.service host/systemd/tehas-dispatch.service \
  test/cycle_test.sh test/dispatch_test.sh test/index_test.sh test/config_test.sh test/cli_test.sh test/install_test.sh \
  host/docs/tools.md host/docs/operating.md host/docs/troubleshooting.md
git commit -m "The dispatcher polls, and each cycle runs as its own unit"
```

---

### Task 3: A workspace and a transcripts directory per issue

This task changes two mounts of the cycle's container. It is a security change under hard rule 4. Nothing new is mounted and no path inside the container changes.

**Files:**
- Modify: `lib/sandbox.sh`, `libexec/work`, `libexec/cycle`, `lib/config.sh` (one comment), `console/tests/console.rs`
- Modify: `host/docs/host.md`, `host/docs/tools.md`, `host/docs/troubleshooting.md`
- Test: `test/work_test.sh`, `test/cycle_test.sh`

**Interfaces:**
- Consumes: `libexec/cycle` from Task 2.
- Produces:
  - `tehas_sandbox_mounts <tree> [<host tree> [<runs dir>]]`: `<tree>` is the path inside the container. The defaults are `<tree>` itself and `~/.pi/agent/ensemble-runs`, which is what `tehas dev` gets.
  - The workspace of issue N on the host is `$TEHAS_WORK_DIR-N`, that is `~/tehas/work/<name>-N`. Inside the container it is at `$TEHAS_WORK_DIR`.
  - The transcripts of issue N on the host are in `~/.pi/agent/ensemble-runs/issue-N`.

- [ ] **Step 1: Change `test/work_test.sh` to pin the new values**

Replace the line `ws=$T/home/tehas/work/widget` with:

```bash
# The workspace of issue 7 on the host, and the path every cycle has it at
# inside its container.
ws=$T/home/tehas/work/widget-7
inside=$T/home/tehas/work/widget
runs=$T/home/.pi/agent/ensemble-runs/issue-7
```

Then:
- in `the image and the working directory come from the settings`: `t_eq "$(after -w)" "$inside"`,
- in the console case: `".jsonl --runs $runs --out $T/home/tehas/live/console-7.txt --since "`,
- in `exactly the documented paths are mounted`, the first two lines of `want` become `$ws:$inside` and `$runs:/home/vscode/.pi/agent/ensemble-runs`,
- in the push check's case: `"$ws:$inside"`.

Add after the case `exactly the documented paths are mounted`:

```bash
t_case "the agents are told the path inside the container, not the host's"
t_contains "$(after -e)" "PI_ENSEMBLE_WORKSPACE_ROOT=$inside"
t_lacks "$(after -e)" "widget-7"

t_case "nothing is made on the host at the path inside the container"
t_eq "$(find "$inside" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "the cycle's transcripts directory is its own and is made for it"
t_eq "$(find "$runs" -maxdepth 0 -type d | wc -l)" 1
```

Add after the case `a commit the cycle made on its own branch does not reach the next cycle`:

```bash
t_case "another issue's workspace and transcripts are left alone"
mkdir -p "$T/home/tehas/work/widget-9" "$T/home/.pi/agent/ensemble-runs/issue-9"
echo keep >"$T/home/tehas/work/widget-9/notes.txt"
echo keep >"$T/home/.pi/agent/ensemble-runs/issue-9/a-1-coder.json"
rm -f "$T"/calls/docker*
t_run "$work 7"
t_eq "$(cat "$T/home/tehas/work/widget-9/notes.txt")" keep
t_eq "$(cat "$T/home/.pi/agent/ensemble-runs/issue-9/a-1-coder.json")" keep
t_lacks "$(cat "$T"/calls/docker.args.*)" "widget-9"
t_lacks "$(cat "$T"/calls/docker.args.*)" "issue-9"
```

Run: `bash test/work_test.sh`
Expected: FAIL on the mounts, the console arguments and the new cases.

- [ ] **Step 2: `lib/sandbox.sh`**

In the header, replace the `tehas_sandbox_mounts` line with:

```bash
#   tehas_sandbox_mounts <tree> [<host tree> [<runs dir>]]
#                                             the -v options
```

Replace the function and the comment above it with:

```bash
# The same mounts as the pi-rukas wrapper, minus the Docker socket, SSH keys,
# gh's configuration and image directories. <tree> is the path inside the
# container. A cycle names the directory on the host that is mounted there and
# a transcripts directory of its own, so every cycle works at one path and
# sees no other cycle's tree or transcripts. A manual session names neither.
# The directories are made first: Docker would create a missing mount source
# as a root-owned directory.
tehas_sandbox_mounts() { # <tree> [<host tree> [<runs dir>]]
  local tree=$1 agent=$HOME/.pi/agent
  local from=${2:-$1} runs=${3:-$HOME/.pi/agent/ensemble-runs}
  mkdir -p "$runs" "$agent/sessions" "$HOME/.vipune" || return 1
  printf '%s\0' -v "$from:$tree" \
    -v "$HOME/.vipune:/home/vscode/.vipune" \
    -v "$runs:/home/vscode/.pi/agent/ensemble-runs" \
    -v "$agent/sessions:/home/vscode/.pi/agent/sessions" \
    -v "$agent/models.json:/home/vscode/.pi/agent/models.json:ro" \
    -v "$agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro" \
    -v pi-ensemble-cache:/home/vscode/.cache \
    -v pi-ensemble-bun-cache:/home/vscode/.bun
}
```

- [ ] **Step 3: `libexec/work`**

Replace the header paragraph that begins `# The workspace is $TEHAS_HOME/work/<name>, a fresh clone` with:

```bash
# The workspace is $TEHAS_HOME/work/<name>-<issue>, a fresh clone made here for
# every cycle. Every cycle has its workspace at the same path inside its
# container, $TEHAS_HOME/work/<name>: the code index is kept under that path,
# so one index serves all cycles. The container never sees the product
# checkout: an agent can write anything into the tree it works in, hooks and
# git configuration included, and the host runs git in the product checkout.
# So the host runs git only in a tree no container has touched, and reads from
# the workspace afterwards nothing but the cycle's state file, as data.
#
# The agents' transcripts go to ~/.pi/agent/ensemble-runs/issue-<issue>, so a
# cycle sees its own and the console shows no other cycle's agents.
```

Replace the three assignments `repo=$TEHAS_WORK_DIR`, `state=...` and (further down) `agent=$HOME/.pi/agent` so that this block reads:

```bash
src=$TEHAS_WORK_REPO
# The workspace inside the container, and the directory on the host behind it.
repo=$TEHAS_WORK_DIR
tree=$TEHAS_WORK_DIR-$issue
image=$PI_ENSEMBLE_IMAGE
limit=$TEHAS_WORK_TIMEOUT
state=$tree/.pi/work-state/$issue.json
log=$TEHAS_HOME/logs/work-$issue-$(date -u +%y%m%d-%H%M%S).jsonl
name=tehas-work-$issue
agent=$HOME/.pi/agent
runs=$agent/ensemble-runs/issue-$issue
```

Change the mounts line to:

```bash
mapfile -d '' -t mounts < <(tehas_sandbox_mounts "$repo" "$tree" "$runs")
```

Replace the workspace block (from the comment `# A fresh workspace for this cycle` to the `fi` that ends `not starting a cycle`) with:

```bash
# A fresh workspace for this cycle, at the tip the product checkout last
# fetched for the branch. What an earlier cycle of this issue left behind goes
# with the old one. Only a directory this command made is ever removed, and
# never another issue's.
works=$(dirname "$tree")
if [ -e "$works" ] && [ ! -f "$works/.tehas-workspaces" ]; then
  echo "tehas work: $works exists and was not made by tehas, not touching it" >&2
  exit 78
fi
if ! { mkdir -p "$works" && : >"$works/.tehas-workspaces" && rm -rf "$tree" &&
  tehas_sandbox_clone "$src" "$tree" "$branch"; }; then
  echo "tehas work: could not make the workspace $tree, not starting a cycle" >&2
  exit 78
fi
```

In the push check, replace `-w "$repo" -v "$repo:$repo"` with `-w "$repo" -v "$tree:$repo"`.

In the console's start, replace `--runs "$agent/ensemble-runs"` with `--runs "$runs"`.

- [ ] **Step 4: The console's pin on that line**

In `console/tests/console.rs`, in `work_calls_the_console_by_the_arguments_it_takes`, replace the second string with:

```rust
        "--runs \"$runs\" --out \"$console\" --since \"$((started - 5))\"",
```

- [ ] **Step 5: Run the work tests**

Run: `bash test/work_test.sh && bash test/dev_test.sh && bash test/index_test.sh`
Expected: `0 failed` for each. `dev_test.sh` and `index_test.sh` are not edited: a manual session's mounts and the indexer's are the same as before.

If cargo is installed, run: `cargo test --locked --quiet --manifest-path console/Cargo.toml`
Expected: `test result: ok`.

- [ ] **Step 6: `libexec/cycle` reads the issue's workspace and tidies it**

In `test/cycle_test.sh`, replace `workdir=$T/home/tehas/work/widget` with `workdir=$T/home/tehas/work/widget-7`, and add before the case `a second cycle for the same issue runs nothing, exit 3`:

```bash
t_case "the workspace of a cycle that left a pull request is removed"
reset
ended handoff commit-pr feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
echo "https://github.com/acme/widget/pull/9" >"$fx/pr.txt"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "so is the workspace of a merged cycle"
reset
ended merged done feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 2>/dev/null | wc -l)" 0

t_case "the workspace of a cycle that stopped is kept for a person to read"
reset
ended aborted develop feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1

t_case "a directory tehas work did not mark is never removed"
reset
ended merged done feature/x
rm -f "$T/home/tehas/work/.tehas-workspaces"
t_run "$cycle 7"
t_eq "$(find "$workdir" -maxdepth 0 | wc -l)" 1

t_case "another issue's workspace is left alone"
reset
ended merged done feature/x
: >"$T/home/tehas/work/.tehas-workspaces"
mkdir -p "$T/home/tehas/work/widget-9"
t_run "$cycle 7"
t_eq "$(find "$T/home/tehas/work/widget-9" -maxdepth 0 | wc -l)" 1
rm -rf "$T/home/tehas/work"
```

In `reset()` of that file, add `"$T/home/tehas/work"` to the `rm -rf`.

Run: `bash test/cycle_test.sh`
Expected: FAIL on the reporting cases (the state file is no longer found) and the new ones.

In `libexec/cycle`, replace `workdir=$TEHAS_WORK_DIR` with:

```bash
# The cycle's workspace on the host; tehas work makes it under this name.
workdir=$TEHAS_WORK_DIR-$issue
```

Add a function after `relabel()`:

```bash
# A workspace is kept only while a person may want to read it. Only a
# directory under one that tehas work marked is removed, as tehas work does.
tidy() {
  if [ -f "$(dirname "$workdir")/.tehas-workspaces" ]; then rm -rf "$workdir"; fi
}
```

and call `tidy` as the last command of the first two branches of the final `if` (after the comment of the merged branch, and after the `gh pr comment ...` of the pull request branch).

Run: `bash test/cycle_test.sh`
Expected: `0 failed`.

- [ ] **Step 7: Comments and the manual**

`lib/config.sh`, the comment above `export TEHAS_WORK_DIR=`:

```bash
    # Where a cycle works, as its container sees it. On the host the tree is
    # $TEHAS_WORK_DIR-<issue>, a fresh clone that tehas work makes for each
    # cycle and the only tree that cycle's container can write. Not a setting.
```

`host/docs/host.md`, in the table of what enters the container, replace the row `| Host paths mounted | ...` with:

```markdown
| Host paths mounted                     | The cycle's own workspace, `~/tehas/work/<name>-<issue>`, at the same path in every container; its own transcripts, `~/.pi/agent/ensemble-runs/issue-<issue>`; `~/.vipune` and Pi's `sessions` (read-write); `models.json`, `mcp.json` (read-only); two caches |
```

Keep the column widths of the neighbouring rows where the text fits.

`host/docs/tools.md`, in the `TEHAS_WORK_REPO` row, replace `The cycle itself works in `~/tehas/work/<name>`, which is not a setting` with `A cycle works in `~/tehas/work/<name>-<issue>`, which is not a setting`.

`host/docs/troubleshooting.md`, replace the two rows:

```markdown
| Where is the cycle now?          | `jq .pipelineState ~/tehas/work/tehas-<issue>/.pi/work-state/<issue>.json`. The workspace of a cycle that left a pull request is removed; one that stopped is kept. |
| What did one agent do?           | Transcripts under `~/.pi/agent/ensemble-runs/issue-<issue>/<date>/`     |
```

- [ ] **Step 8: Gate and commit**

Run: `scripts/verify.sh`
Expected: every line `ok` (or `skip`).

```bash
git add lib/sandbox.sh lib/config.sh libexec/work libexec/cycle console/tests/console.rs \
  test/work_test.sh test/cycle_test.sh host/docs/host.md host/docs/tools.md host/docs/troubleshooting.md
git commit -m "A workspace and a transcripts directory per issue"
```

The commit body says: `Security: two mounts of the cycle's container change their source on the host. The workspace comes from ~/tehas/work/<name>-<issue> and the transcripts from ~/.pi/agent/ensemble-runs/issue-<issue>. The paths inside the container are the same and nothing new is mounted. A cycle no longer sees the transcripts of other cycles. test/work_test.sh pins the new values.`

---

### Task 4: The live view

**Files:**
- Create: `lib/live.sh`
- Modify: `libexec/work`, `libexec/health`, `libexec/summary`, `libexec/dev`, `install.sh`
- Modify: `host/docs/tools.md`
- Test: `test/work_test.sh`, `test/status_test.sh`, `test/dev_test.sh`, `test/install_test.sh`

**Interfaces:**
- Produces:
  - `tehas_live_assemble` in `lib/live.sh`. Needs `TEHAS_HOME`. Reads `$TEHAS_HOME/live/cycles/*.json`, writes `$TEHAS_HOME/live/running.json`.
  - `live/cycles/<issue>.json`: `{issue, running, status, step, statusText, startedAt, updatedAt, console}`, the object `running.json` held before.
  - `live/running.json`: `{"cycles": [<running cycles, oldest first>], "last": <the cycle that finished most recently, or null>}`.
  - `health.json`: as before, plus `checks.cycles`, an array of the running cycles in the shape of `checks.cycle`. `checks.cycle` is the first running cycle, or `last`.

- [ ] **Step 1: Tests for the work script's live files**

In `test/work_test.sh`, in the two status-line cases, read the finished cycle from `last`:

```bash
t_eq "$(jq -r .last.statusText "$T/home/tehas/live/running.json")" "step 3 of the [masked]"
```

```bash
t_eq "$(jq -r '.last.statusText, .last.status' "$T/home/tehas/live/running.json" | tr '\n' ,)" ",running,"
```

Add after the second of them (after the stubs are put back):

```bash
live=$T/home/tehas/live
other() { # issue running status updatedAt: the live file of another cycle
  printf '{"issue":%s,"running":%s,"status":"%s","step":"develop","statusText":"","startedAt":%s,"updatedAt":%s,"console":"console-%s.txt"}\n' \
    "$1" "$2" "$3" "$(($4 - 60))" "$4" "$1" >"$live/cycles/$1.json"
}

t_case "the cycle writes a live file of its own, and the view has it as the last one"
t_run "$work 7"
t_eq "$(jq -c '[.issue, .running, .console]' "$live/cycles/7.json")" '[7,false,"console-7.txt"]'
t_eq "$(jq -c '[(.cycles | length), .last.issue]' "$live/running.json")" "[0,7]"

t_case "another cycle that is running is in the view beside it"
other 9 true running "$(date +%s)"
t_run "$work 7"
t_eq "$(jq -c '[.cycles[].issue, .last.issue]' "$live/running.json")" "[9,7]"
t_eq "$(jq -r .cycles[0].console "$live/running.json")" "console-9.txt"

t_case "a finished cycle that is not the last one leaves the view and the disk"
other 5 false handoff 1000
t_run "$work 7"
t_eq "$(find "$live/cycles" -name '*.json' -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "7.json 9.json "
t_eq "$(jq -r .last.issue "$live/running.json")" 7

t_case "a cycle that has written nothing for two minutes is no longer shown as running"
other 9 true running "$(($(date +%s) - 300))"
t_run "$work 7"
t_eq "$(jq -c '[(.cycles | length), .last.issue]' "$live/running.json")" "[0,7]"
rm -rf "$live/cycles"
```

Run: `bash test/work_test.sh`
Expected: FAIL on these and on the two status-line cases.

- [ ] **Step 2: `lib/live.sh`**

```bash
# shellcheck shell=bash
# The live view of the cycles: $TEHAS_HOME/live/running.json, read by the
# factory floor, the plain dashboard, tehasbot health and tehas dev.
#
# Each cycle writes a file of its own, live/cycles/<issue>.json, every ten
# seconds: one file cannot have several writers. The view is put together
# from those files:
#
#   {"cycles": [<each running cycle, oldest first>],
#    "last": <the cycle that finished most recently, or null>}
#
#   tehas_live_assemble   write live/running.json; returns 1 if it could not
#
# Called by tehas work each time it writes its own file, and by tehasbot
# health. Two callers at once can leave the view one tick old; the next tick
# corrects it. Needs TEHAS_HOME.

tehas_live_assemble() {
  local live=$TEHAS_HOME/live f files=() now keep tmp
  now=$(date +%s)
  for f in "$live"/cycles/*.json; do
    [ -e "$f" ] || continue
    # A cycle writes every ten seconds. Two minutes of silence is a cycle
    # that died without saying so; it must not stay on the page as running.
    if jq -e --argjson now "$now" '.running == true and ((.updatedAt // 0) < $now - 120)' "$f" >/dev/null 2>&1; then
      if jq -c '.running = false | .status = "lost"' "$f" >"$f.new" 2>/dev/null; then mv "$f.new" "$f"; fi
    fi
    files+=("$f")
  done
  # Nothing written in this shape yet: leave what an earlier version wrote.
  [ "${#files[@]}" -gt 0 ] || return 0
  tmp=$live/running.json.$$
  if ! jq -c -s '{cycles: (map(select(.running == true)) | sort_by(.startedAt, .issue)),
                   last: (map(select(.running != true)) | sort_by(.updatedAt) | last)}' "${files[@]}" >"$tmp" 2>/dev/null; then
    rm -f "$tmp"
    return 1
  fi
  mv "$tmp" "$live/running.json" || return 1
  # Only the last finished cycle is shown, so only its file is kept.
  keep=$(jq -r '.last.issue // empty' "$live/running.json" 2>/dev/null)
  for f in "${files[@]}"; do
    [ "${f##*/}" = "$keep.json" ] && continue
    if jq -e '.running != true' "$f" >/dev/null 2>&1; then rm -f "$f"; fi
  done
  return 0
}
```

- [ ] **Step 3: `libexec/work` writes its own file**

Source the library after `lib/sandbox.sh`:

```bash
# shellcheck source=lib/live.sh
. "$here/../lib/live.sh"
```

Replace `mkdir -p "$live"` with `mkdir -p "$live/cycles"`. In `now_file`, replace the last two lines (the `>"$live/running.json.new" ... && mv ...`) with:

```bash
    >"$live/cycles/$issue.json.new" 2>/dev/null && mv "$live/cycles/$issue.json.new" "$live/cycles/$issue.json"
  tehas_live_assemble
```

Change the comment above `live=$TEHAS_HOME/live` to `# For the dashboard: a readable console of this cycle and its own "now" file.`

Run: `bash test/work_test.sh`
Expected: `0 failed`.

- [ ] **Step 4: Tests for health, the summary and `tehas dev`**

In `test/status_test.sh`, replace the helper `cycle()` with:

```bash
one() { # issue running status step: one cycle as the work script writes it
  printf '{"issue":%s,"running":%s,"status":"%s","step":"%s","startedAt":%s,"updatedAt":%s}' \
    "$1" "$2" "$3" "$4" "$(($(date +%s) - 600))" "$(($(date +%s) - 180))"
}
cycle() { # running status step: issue 12, running or the last one
  mkdir -p "$live"
  if [ "$1" = true ]; then
    printf '{"cycles":[%s],"last":null}\n' "$(one 12 "$@")" >"$live/running.json"
  else
    printf '{"cycles":[],"last":%s}\n' "$(one 12 "$@")" >"$live/running.json"
  fi
}
```

Add these cases after `summary: a failed cycle is exit 1, and still printed`:

```bash
t_case "summary: two running cycles are both named, each on a line"
reset
mkdir -p "$live"
printf '{"cycles":[%s,%s],"last":null}\n' "$(one 12 true running develop)" "$(one 14 true running plan)" >"$live/running.json"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #12, #14."
t_contains "$t_out" "cycle     #12 running, step develop"
t_contains "$t_out" "cycle     #14 running, step plan"
t_eq "$(jq -c '[.checks.cycle.issue, (.checks.cycles | map(.issue))]' "$live/health.json")" "[12,[12,14]]"

t_case "summary: the last cycle failed, whatever is running beside it"
reset
mkdir -p "$live"
printf '{"cycles":[%s],"last":%s}\n' "$(one 14 true running plan)" "$(one 12 false aborted develop)" >"$live/running.json"
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "factory   fail: The last cycle failed: #12 ended as aborted."

t_case "health reads the one object an earlier version wrote: a running cycle"
reset
mkdir -p "$live"
one 12 true running develop >"$live/running.json"
t_run "$status && $summary"
t_eq "$t_rc" 0
t_contains "$t_out" "factory   pass: Working on #12."

t_case "health reads the one object an earlier version wrote: a failed cycle stays failed"
reset
mkdir -p "$live"
one 12 false aborted develop >"$live/running.json"
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #12 ended as aborted."

t_case "a cycle that stopped writing is lost, and the factory says so"
reset
mkdir -p "$live/cycles"
printf '{"issue":9,"running":true,"status":"running","step":"develop","startedAt":%s,"updatedAt":%s}\n' \
  "$(($(date +%s) - 900))" "$(($(date +%s) - 300))" >"$live/cycles/9.json"
t_run "$status; $summary"
t_eq "$t_rc" 1
t_contains "$t_out" "The last cycle failed: #9 ended as lost."
t_eq "$(jq -c '[(.cycles | length), .last.status]' "$live/running.json")" '[0,"lost"]'
```

In `test/dev_test.sh`, replace the whole case `a running cycle is announced, and the session starts` (up to the next `t_case`) with:

```bash
t_case "a running cycle is announced, and the session starts"
mkdir -p "$T/home/tehas/live"
echo '{"cycles":[{"issue":12,"running":true}],"last":null}' >"$T/home/tehas/live/running.json"
again
t_run "$dev"
t_eq "$t_rc" 0
t_contains "$t_out" "a cycle is running on issue 12"
t_eq "$(containers)" 1
echo '{"cycles":[],"last":{"issue":12,"running":false}}' >"$T/home/tehas/live/running.json"
t_run "$dev"
t_lacks "$t_out" "issue 12"

t_case "several running cycles are all named"
echo '{"cycles":[{"issue":12,"running":true},{"issue":14,"running":true}],"last":null}' >"$T/home/tehas/live/running.json"
t_run "$dev"
t_contains "$t_out" "cycles are running on issues 12, 14"

t_case "the one object an earlier version wrote is still read"
echo '{"issue":12,"running":true}' >"$T/home/tehas/live/running.json"
t_run "$dev"
t_contains "$t_out" "a cycle is running on issue 12"
echo '{"issue":12,"running":false}' >"$T/home/tehas/live/running.json"
t_run "$dev"
t_lacks "$t_out" "issue 12"
```

Run: `bash test/status_test.sh; bash test/dev_test.sh`
Expected: FAIL on the new cases and on every existing `summary:` case that has a cycle.

- [ ] **Step 5: `libexec/health`**

In the header, replace `running.json (the current or last cycle from tehas work)` with `running.json (the running cycles and the last one, from tehas work)`.

After `mkdir -p "$live"` add:

```bash
here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
# shellcheck source=lib/live.sh
. "$here/../lib/live.sh"
# A cycle that died without a word stops being shown as running here.
tehas_live_assemble 2>/dev/null || true
```

Replace from `queue=null` to the end of the `jq -n` program (up to `>"$live/health.json.new"`) with:

```bash
queue=null
view=null
[ -s "$status_file" ] && queue=$(jq -c . "$status_file" 2>/dev/null || echo null)
[ -s "$running_file" ] && view=$(jq -c . "$running_file" 2>/dev/null || echo null)

jq -n --argjson queue "$queue" --argjson view "$view" --arg now "$(date -u +%FT%TZ)" '
  def iso: if type == "number" then todate else null end;
  def brief: {
    issue, running: (.running // false), status, step,
    startedAt: (.startedAt | iso), time: (.updatedAt | iso)
  };
  # The view tehas work writes, or the one object an earlier version wrote.
  (if ($view | type) != "object" then {cycles: [], last: null}
   elif ($view | has("cycles")) then $view
   elif $view.running == true then {cycles: [$view], last: null}
   else {cycles: [], last: $view} end) as $v
  | ($v.cycles // []) as $running
  | $v.last as $last
  | ($last != null and (($last.status // "") | IN("merged", "handoff") | not)) as $failed
  | (if $queue == null then "fail"
     elif $failed then "fail"
     elif $queue.paused then "warn"
     else "pass" end) as $status
  | {
      status: $status,
      time: $now,
      output: (if $queue == null then "No queue snapshot yet: the dispatcher has not written one."
               elif $failed then "The last cycle failed: #\($last.issue) ended as \($last.status)."
               elif $queue.paused then "The factory is paused."
               elif ($running | length) > 0 then "Working on \($running | map("#\(.issue)") | join(", "))."
               else "Idle, taking work." end),
      checks: {
        queue: (if $queue == null then null else {
          time: $queue.generatedAt,
          paused: ($queue.paused // false),
          inLine: ($queue.queued | length),
          building: ($queue.running | length),
          review: ($queue.awaiting | length),
          stopped: ($queue.stopped | length)
        } end),
        cycle: (($running[0] // $last) | if . == null then null else brief end),
        cycles: ($running | map(brief))
      }
    }' >"$live/health.json.new" 2>/dev/null || exit 1
```

`libexec/health` now needs `lib/live.sh` beside it. If a test copies `libexec/health` into a scratch tree of its own (search `test/` for `libexec/health`), copy `lib/live.sh` there too.

- [ ] **Step 6: `libexec/summary` and `libexec/dev`**

In `libexec/summary`, replace the last filter of the `jq -r` program (the one that begins `(.checks.cycle // null | if . == null then "cycle     none yet"`) with:

```
  ((if ((.checks.cycles // []) | length) > 1 then .checks.cycles[] else (.checks.cycle // null) end)
   | if . == null then "cycle     none yet"
     elif .running then "cycle     #\(.issue) running, step \(.step // "-"), started \(.startedAt // "" | age)"
     else "cycle     #\(.issue) \(.status // "unknown") at step \(.step // "-"), \(.time // "" | age)" end)
```

In `libexec/dev`, replace the three lines that read `running.json` and print the notice with:

```bash
# The running cycles, from the view or from the one object an earlier version wrote.
running=$(jq -r '(if has("cycles") then .cycles else [select(.running == true)] end)[] | .issue' \
  "$TEHAS_HOME/live/running.json" 2>/dev/null | paste -sd, - | sed 's/,/, /g') || running=""
case "$running" in
  '') ;;
  *,*) echo "tehas dev: cycles are running on issues $running; the session shares the model endpoint with them" ;;
  *) echo "tehas dev: a cycle is running on issue $running; the session shares the model endpoint with it" ;;
esac
```

Run: `bash test/status_test.sh && bash test/dev_test.sh`
Expected: `0 failed` for both.

- [ ] **Step 7: `install.sh`**

In `test/install_test.sh`, add `./tehas/lib/live.sh` to `files=` after `./tehas/lib/cycles.sh`. Run it and see `a fresh install: exactly these files` fail. In `install.sh`, add after the `lib/cycles.sh` line:

```bash
  "lib/live.sh|$TEHAS_HOME/lib/live.sh|644"
```

Run: `bash test/install_test.sh`
Expected: `0 failed`.

- [ ] **Step 8: The manual**

`host/docs/tools.md`:
- In `## tehasbot status and the console`, replace `The work script's own status line in `~/tehas/live/running.json` goes through` with `The work script's own status line, in `~/tehas/live/cycles/<issue>.json` and from there in `~/tehas/live/running.json`, goes through`.
- In `## tehasbot health`, replace the first sentence's `the queue snapshot and the current or last cycle` with `the queue snapshot, the running cycles and the last one that finished`, and replace the paragraph that begins `` `output` says the same in a sentence `` with:

```markdown
`output` says the same in a sentence. `checks.queue` has the snapshot time and
the counts in line, building, in review and stopped. `checks.cycles` has the
issue, status, step and times of every running cycle; `checks.cycle` is the
first of them, or the cycle that finished last when none is running.

`fail` is about the cycle that finished most recently, whatever is running
beside it. A cycle that has written nothing for two minutes is recorded as
ended with the status `lost`, which is a failure.
```

- [ ] **Step 9: Gate and commit**

Run: `scripts/verify.sh --full`
Expected: every line `ok` (or `skip` where a tool is missing).

```bash
git add lib/live.sh libexec/work libexec/health libexec/summary libexec/dev install.sh \
  test/work_test.sh test/status_test.sh test/dev_test.sh test/install_test.sh host/docs/tools.md
git commit -m "The live view lists every running cycle"
```

---

### Task 5: The console's tabs

The console is shown on the factory floor (`host/fun/index.html`, drawn by `host/fun/game-view.js`) and on the plain dashboard (`host/docs/dashboard.frag`). Both get the same row of tabs.

**Files:**
- Create: `host/fun/console-tabs.js`, `test/console_tabs_test.mjs`
- Modify: `host/fun/game-view.js`, `host/fun/index.html`, `host/fun/game-view.css`, `host/docs/dashboard.frag`, `host/fun/manual.css`, `DESIGN.md`, `host/docs/operating.md`, `host/docs/features.md`

**Interfaces:**
- Consumes: `live/running.json` in the shape of Task 4, or the one object an earlier version wrote.
- Produces: `globalThis.TEHAS_CONSOLE_TABS` with
  - `view(now, selected)` returns `{ tabs, shown, running, failure }`. `tabs` is an array of `{ issue: number, running: boolean, cycle: object }`, running cycles first. `shown` is the tab whose issue is `selected`, else the first tab, else `null`. `running` is the number of running cycles. `failure` is `{ issue, status }` when the last cycle ended as anything but `merged` or `handoff`, else `null`.
  - `consoleFile(cycle)` returns the cycle's console file name, or `null` if it is not a plain file name.
  - `render(row, v, onSelect)` draws the tabs of `v` into the element `row` and calls `onSelect(issue)` on a click.
  - `keys(row, onSelect)` adds arrow, Home and End key handling to `row`, once per page.

- [ ] **Step 1: Write `test/console_tabs_test.mjs`**

```js
// Tests view() of the console's tabs: the pure step from live/running.json
// to which cycles can be shown and which one is.
//
//   node --test test/console_tabs_test.mjs
import assert from "node:assert/strict";
import { test } from "node:test";
import "../host/fun/console-tabs.js";

const { view, consoleFile } = globalThis.TEHAS_CONSOLE_TABS;

const cycle = (issue, over = {}) => ({
  issue, running: true, status: "running", step: "develop", statusText: "",
  startedAt: 1000, updatedAt: 2000, console: `console-${issue}.txt`, ...over,
});
const ended = (issue, status = "handoff") => cycle(issue, { running: false, status });
const issues = (v) => v.tabs.map((t) => t.issue);

test("no file, or nothing in it, is no tab and nothing shown", () => {
  for (const now of [null, undefined, {}, { cycles: [], last: null }]) {
    const v = view(now, null);
    assert.deepEqual(v.tabs, []);
    assert.equal(v.shown, null);
    assert.equal(v.running, 0);
    assert.equal(v.failure, null);
  }
});

test("one running cycle is one tab, and it is shown", () => {
  const v = view({ cycles: [cycle(12)], last: null }, null);
  assert.deepEqual(issues(v), [12]);
  assert.equal(v.shown.issue, 12);
  assert.equal(v.shown.running, true);
  assert.equal(v.shown.cycle.console, "console-12.txt");
  assert.equal(v.running, 1);
});

test("two running cycles are two tabs in the order given, the first shown", () => {
  const v = view({ cycles: [cycle(12), cycle(14)], last: null }, null);
  assert.deepEqual(issues(v), [12, 14]);
  assert.equal(v.shown.issue, 12);
  assert.equal(v.running, 2);
});

test("the selected tab is shown", () => {
  const v = view({ cycles: [cycle(12), cycle(14)], last: null }, 14);
  assert.equal(v.shown.issue, 14);
});

test("a selection that is no longer listed falls back to the first tab", () => {
  const v = view({ cycles: [cycle(12), cycle(14)], last: null }, 99);
  assert.equal(v.shown.issue, 12);
});

test("the last cycle is a tab after the running ones, marked as ended", () => {
  const v = view({ cycles: [cycle(12)], last: ended(7) }, null);
  assert.deepEqual(issues(v), [12, 7]);
  assert.deepEqual(v.tabs.map((t) => t.running), [true, false]);
  assert.equal(view({ cycles: [cycle(12)], last: ended(7) }, 7).shown.issue, 7);
});

test("with nothing running the last cycle is the one tab", () => {
  const v = view({ cycles: [], last: ended(7) }, null);
  assert.deepEqual(issues(v), [7]);
  assert.equal(v.shown.running, false);
  assert.equal(v.running, 0);
});

test("an issue that is running again has one tab, the running one", () => {
  const v = view({ cycles: [cycle(7)], last: ended(7, "aborted") }, null);
  assert.deepEqual(issues(v), [7]);
  assert.equal(v.shown.running, true);
});

test("the same issue listed twice is one tab", () => {
  assert.deepEqual(issues(view({ cycles: [cycle(12), cycle(12)], last: null }, null)), [12]);
});

test("a last cycle that ended badly is the failure, whatever is running", () => {
  assert.deepEqual(view({ cycles: [cycle(12)], last: ended(7, "aborted") }, null).failure, { issue: 7, status: "aborted" });
  assert.deepEqual(view({ cycles: [], last: ended(7, "lost") }, null).failure, { issue: 7, status: "lost" });
  assert.equal(view({ cycles: [], last: ended(7, "merged") }, null).failure, null);
  assert.equal(view({ cycles: [], last: ended(7, "handoff") }, null).failure, null);
});

test("a long status is cut, as it is drawn on the floor", () => {
  assert.equal(view({ cycles: [], last: ended(7, "x".repeat(100)) }, null).failure.status.length, 40);
});

test("the one object an earlier version wrote is read: a running cycle", () => {
  const v = view(cycle(12), null);
  assert.deepEqual(issues(v), [12]);
  assert.equal(v.running, 1);
  assert.equal(v.failure, null);
});

test("the one object an earlier version wrote is read: an ended cycle", () => {
  const v = view(ended(12, "aborted"), null);
  assert.deepEqual(issues(v), [12]);
  assert.equal(v.running, 0);
  assert.deepEqual(v.failure, { issue: 12, status: "aborted" });
});

test("what is not a cycle is left out, and nothing throws", () => {
  const junk = [null, 7, "12", [], {}, { issue: "12" }, { issue: 0 }, { issue: -3 }, { issue: 1.5 }, { issue: null }];
  const v = view({ cycles: [...junk, cycle(12)], last: "nope" }, null);
  assert.deepEqual(issues(v), [12]);
  for (const now of [7, "text", [], [cycle(12)], { cycles: "many" }, { cycles: null, last: 3 }]) {
    assert.deepEqual(view(now, null).tabs, []);
  }
});

test("a console file is a plain file name or nothing", () => {
  assert.equal(consoleFile(cycle(12)), "console-12.txt");
  for (const name of ["../secret", "a/b.txt", "", 7, null, undefined, "console 12.txt"]) {
    assert.equal(consoleFile(cycle(12, { console: name })), null);
  }
});
```

Run: `node --test test/console_tabs_test.mjs`
Expected: FAIL, `Cannot find module ... console-tabs.js`.

- [ ] **Step 2: Write `host/fun/console-tabs.js`**

```js
// The tabs of the console: which cycles can be shown and which one is.
// See DESIGN.md, "Production Console".
//
// The factory runs up to TEHAS_TICKET_CONCURRENCY cycles side by side, and
// each has a console of its own. live/running.json lists the running cycles
// and the one that finished last. view() turns that file into the tabs. It
// has no side effects and draws nothing, so it is tested without a browser
// (test/console_tabs_test.mjs). The factory floor (game-view.js) and the
// plain dashboard (dashboard.frag) both draw what it returns, with render().
//
// The file is read from the network and its text is untrusted: a tab's label
// is an issue number, set as text.
(() => {
  const FINE = ["merged", "handoff"];
  const isCycle = (c) =>
    c !== null && typeof c === "object" && !Array.isArray(c) && Number.isSafeInteger(c.issue) && c.issue > 0;

  // The running cycles and the last one, from the file as it is now or from
  // the one object an earlier version wrote.
  function cycles(now) {
    if (now === null || typeof now !== "object" || Array.isArray(now)) return { running: [], last: null };
    if (!("cycles" in now) && !("last" in now)) {
      if (!isCycle(now)) return { running: [], last: null };
      return now.running === true ? { running: [now], last: null } : { running: [], last: now };
    }
    const seen = new Set();
    const running = (Array.isArray(now.cycles) ? now.cycles : []).filter((c) => {
      if (!isCycle(c) || seen.has(c.issue)) return false;
      seen.add(c.issue);
      return true;
    });
    return { running, last: isCycle(now.last) ? now.last : null };
  }

  function view(now, selected) {
    const { running, last } = cycles(now);
    const tabs = running.map((cycle) => ({ issue: cycle.issue, running: true, cycle }));
    if (last && !running.some((c) => c.issue === last.issue)) tabs.push({ issue: last.issue, running: false, cycle: last });
    const shown = tabs.find((t) => t.issue === selected) || tabs[0] || null;
    const failure = last && !FINE.includes(String(last.status))
      ? { issue: last.issue, status: String(last.status).slice(0, 40) } : null;
    return { tabs, shown, running: running.length, failure };
  }

  // The name is put into a path under /live/.
  const consoleFile = (cycle) =>
    typeof cycle.console === "string" && /^[\w-][\w.-]*$/.test(cycle.console) ? cycle.console : null;

  // Draws the row. The buttons are made again only when the tabs change, so
  // a refresh every ten seconds does not take the keyboard's place away.
  function render(row, v, onSelect) {
    const key = v.tabs.map((t) => t.issue + (t.running ? "r" : "e")).join(" ");
    row.hidden = v.tabs.length < 2;
    if (row.dataset.tabs !== key) {
      const focused = row.contains(document.activeElement);
      row.dataset.tabs = key;
      row.replaceChildren(...v.tabs.map((t) => {
        const b = document.createElement("button");
        b.type = "button";
        b.setAttribute("role", "tab");
        b.dataset.issue = String(t.issue);
        b.textContent = "#" + t.issue + (t.running ? "" : " ended");
        b.addEventListener("click", () => onSelect(t.issue));
        return b;
      }));
      if (focused && v.shown) {
        const b = [...row.children].find((c) => Number(c.dataset.issue) === v.shown.issue);
        if (b) b.focus();
      }
    }
    for (const b of row.children) {
      const on = v.shown !== null && Number(b.dataset.issue) === v.shown.issue;
      b.setAttribute("aria-selected", String(on));
      b.tabIndex = on ? 0 : -1;
    }
  }

  // The arrow keys move between tabs and select, as in any tab list.
  function keys(row, onSelect) {
    row.addEventListener("keydown", (event) => {
      const tabs = [...row.children], at = tabs.indexOf(document.activeElement);
      if (at < 0) return;
      const to = { ArrowRight: at + 1, ArrowLeft: at - 1, Home: 0, End: tabs.length - 1 }[event.key];
      if (to === undefined) return;
      event.preventDefault();
      const next = tabs[(to + tabs.length) % tabs.length];
      next.focus();
      onSelect(Number(next.dataset.issue));
    });
  }

  globalThis.TEHAS_CONSOLE_TABS = { view, consoleFile, render, keys };
})();
```

Run: `node --test test/console_tabs_test.mjs`
Expected: `# pass 15`, `# fail 0`.

- [ ] **Step 3: The factory floor**

`host/fun/index.html`: add the script before `game-view.js`, and the row and a role inside the console:

```html
  <script src="/fun/console-tabs.js"></script>
  <script src="/fun/game-view.js"></script>
```

```html
      <div class="console-body">
        <div class="console-tabs" id="console-tabs" role="tablist" aria-label="Cycles" hidden></div>
        <p class="console-cycle" id="console-cycle">Looking for a running cycle.</p>
        <p class="console-step" id="console-step"></p>
        <pre id="console-out" role="tabpanel" tabindex="0" aria-label="Console of the selected cycle"></pre>
```

`host/fun/game-view.css`: add after the `.console-step` rule:

```css
/* One tab per cycle, shown when there is more than one to choose from. */
.console-tabs { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 16px; }
.console-tabs[hidden] { display: none; }
.console-tabs button {
  min-height: 48px; padding: 0 16px;
  font: inherit; color: inherit; text-transform: uppercase;
  background: var(--bone); border: 4px solid var(--outline); cursor: pointer;
}
.console-tabs button[aria-selected="true"] { background: var(--brass); }
```

`host/fun/game-view.js`: replace everything from the comment `// The production console. Unlike the tickets above, this is live` down to and including the closing brace of `refreshConsole()` (the line before `async function tailConsole() {`) with:

```js
  // The production console. Unlike the tickets above, this is live: it reads the
  // files the dispatcher writes under /live/, the same ones the plain dashboard
  // reads, and tails the selected cycle's console with Range requests so it only
  // fetches what is new. Console text is untrusted and only ever appended as text.
  // Several cycles can run side by side: console-tabs.js says which there are.
  const TABS = globalThis.TEHAS_CONSOLE_TABS;
  const live = { file: null, offset: 0, running: false, titles: new Map(), now: null, selected: null };
  const duration = (secs) => {
    const m = Math.max(0, Math.floor(secs / 60)), h = Math.floor(m / 60);
    return h > 0 ? `${h} h ${m % 60} min` : `${m} min`;
  };

  // The tag describes the cycle that is shown. The door, the monitor's lamp
  // and the console tag's blink say whether any cycle is running at all.
  function consoleState(state, text, doorText, working) {
    $("console-state").dataset.state = state;
    $("console-state").textContent = text;
    $("door-console").textContent = doorText;
    $("door-console-link").classList.toggle("is-alert", working);
    document.body.dataset.factory = working ? "live" : state;
  }

  async function refreshConsole() {
    if (document.hidden) return;
    let now = null;
    try {
      const r = await fetch("/live/running.json", { cache: "no-store" });
      if (r.ok) now = await r.json();
    } catch { /* unreachable: handled in showConsole */ }
    live.now = now;
    if (TABS.view(now, null).tabs.length) {
      try {
        const r = await fetch("/live/status.json", { cache: "no-store" });
        if (r.ok) {
          const s = await r.json();
          for (const key of ["running", "queued", "stopped", "awaiting", "completed"]) {
            for (const t of Array.isArray(s[key]) ? s[key] : []) live.titles.set(Number(t.number), String(t.title || ""));
          }
        }
      } catch { /* titles are a nicety */ }
    }
    showConsole();
  }

  function selectConsole(issue) {
    live.selected = issue;
    showConsole();
  }

  function showConsole() {
    const v = TABS.view(live.now, live.selected);
    const before = failure && failure.issue + failure.status;
    failure = v.failure;
    if ((failure && failure.issue + failure.status) !== before) renderFloor();
    TABS.render($("console-tabs"), v, selectConsole);
    const tab = v.shown;
    if (!tab) {
      live.file = null;
      live.running = false;
      consoleState("off", "NO CONSOLE", "NOTHING TO SHOW", false);
      $("console-cycle").textContent = "No cycle to show.";
      $("console-step").textContent = "Either no cycle has run yet, or this page cannot reach the factory's live files.";
      $("console-out").textContent = "";
      $("console-out").dataset.empty = "The console of a running cycle appears here.";
      $("console-note").textContent = "";
      return;
    }
    const now = tab.cycle;
    live.running = tab.running;
    if (live.running) live.idle = false;
    const title = live.titles.get(now.issue);
    $("console-cycle").textContent = (live.running ? "Working on #" : "Last cycle: #") + now.issue + (title ? "  " + title : "");
    const elapsed = (live.running ? Date.now() / 1000 : Number(now.updatedAt)) - Number(now.startedAt);
    $("console-step").textContent = live.running
      ? `${now.statusText || "step " + now.step} (${duration(elapsed)} so far)`
      : `Ended as ${now.status} at step ${now.step} after ${duration(elapsed)}.`;
    const door = v.running === 0 ? "IDLE" : v.running === 1 ? `WORKING ON #${v.tabs[0].issue}` : `WORKING ON ${v.running} ISSUES`;
    consoleState(live.running ? "live" : "ended", live.running ? "LIVE" : "ENDED", door, v.running > 0);

    const file = TABS.consoleFile(now);
    if (file !== live.file) {
      live.file = file;
      live.offset = 0;
      live.idle = false;
      $("console-out").textContent = "";
      $("console-out").dataset.empty = file ? "Waiting for output." : "This cycle has no console file.";
      tailConsole();
    }
  }
```

After the existing `$("console-follow").addEventListener(...)` block, add:

```js
  TABS.keys($("console-tabs"), selectConsole);
```

Leave `tailConsole()` and the timers as they are.

- [ ] **Step 4: The plain dashboard**

`host/docs/dashboard.frag`: put the row inside the `now` section, above its title:

```html
<section class="now" id="now" hidden>
  <div class="console-tabs" id="tabs" role="tablist" aria-label="Cycles" hidden></div>
  <h2 id="now-title"></h2>
  <div class="step" id="now-step"></div>
  <pre id="console" role="tabpanel" aria-label="Console of the selected cycle" tabindex="0"></pre>
```

Add `<script src="/fun/console-tabs.js"></script>` on the line before the `<script>` that opens the page's own code.

In that code, replace from `let consoleFile = null, offset = 0, running = null;` down to and including the closing brace of `refresh()` (the line before `async function tail() {`) with:

```js
  // Several cycles can run side by side: console-tabs.js says which there are
  // and which one is shown. running is the shown cycle while it runs.
  const TABS = globalThis.TEHAS_CONSOLE_TABS;
  let consoleFile = null, offset = 0, running = null, selected = null, lastStatus = null, lastNow = null;

  async function refresh() {
    let s, now;
    try {
      s = await (await fetch("/live/status.json", { cache: "no-store" })).json();
    } catch {
      $("fresh").textContent = "no status yet: the dispatcher has not written one";
      return;
    }
    try {
      now = await (await fetch("/live/running.json", { cache: "no-store" })).json();
    } catch {
      now = null;
    }
    lastStatus = s;
    lastNow = now;
    $("fresh").textContent = "queue read " + ago(s.generatedAt);
    $("all").href = s.links.all;

    list("queued", s.queued, s.links.queued, "Nothing is waiting. Label an issue tehas:ready.", "updatedAt");
    list("awaiting", s.awaiting, s.links.awaiting, "No pull request is waiting.", "updatedAt");
    list("stopped", s.stopped, s.links.stopped, "Nothing is stopped.", "updatedAt");
    list("completed", s.completed, s.links.completed, "Nothing completed yet.", "closedAt");
    show();
  }

  function select(issue) {
    selected = issue;
    show();
  }

  // The selected cycle, running or the last one, and its console.
  function show() {
    const s = lastStatus;
    if (!s) return;
    const v = TABS.view(lastNow, selected);
    const pill = $("state");
    if (v.running) { pill.className = "pill run"; pill.textContent = v.running > 1 ? "working on " + v.running : "working"; }
    else if (s.paused) { pill.className = "pill pause"; pill.textContent = "paused"; }
    else { pill.className = "pill idle"; pill.textContent = "idle"; }

    TABS.render($("tabs"), v, select);
    const tab = v.shown;
    running = tab && tab.running ? tab.cycle : null;
    $("now").hidden = !tab;
    if (!tab) return;
    const shown = tab.cycle;
    const issue = [...s.running, ...s.queued, ...s.stopped, ...s.awaiting, ...s.completed]
      .find((i) => i.number === shown.issue);
    $("now-title").textContent = (running ? "Working on #" : "Last cycle: #") + shown.issue +
      (issue ? "  " + issue.title : "");
    const elapsed = (running ? Date.now() / 1000 : shown.updatedAt) - shown.startedAt;
    $("now-step").textContent = running
      ? (shown.statusText || "step " + shown.step) + "   (" + dur(elapsed) + " so far)"
      : "ended as " + shown.status + " at step " + shown.step + " after " + dur(elapsed);
    const file = TABS.consoleFile(shown);
    if (file !== consoleFile) {
      consoleFile = file;
      offset = 0;
      $("console").textContent = "";
      tail();
    }
  }
```

After `refresh();` near the end of the script, add `TABS.keys($("tabs"), select);`.

`host/fun/manual.css`: add after the `.page #console` rule:

```css
/* One tab per cycle, shown when there is more than one to choose from. */
.page .console-tabs { display: flex; flex-wrap: wrap; gap: 8px; margin-bottom: 16px; }
.page .console-tabs[hidden] { display: none; }
.page .console-tabs button {
  min-height: 48px; padding: 0 16px;
  font: inherit; color: inherit; text-transform: uppercase;
  background: var(--bone-shade); border: 4px solid var(--outline); cursor: pointer;
}
.page .console-tabs button[aria-selected="true"] { background: var(--brass); }
```

- [ ] **Step 5: See it in a browser**

The pages need only `/fun/` and `/live/` from a web server. In a scratch directory:

```bash
s=$(mktemp -d) && mkdir "$s/live" && ln -s "$PWD/host/fun" "$s/fun"
now=$(date +%s)
printf 'first cycle\nline two\n' >"$s/live/console-12.txt"
printf 'second cycle\n' >"$s/live/console-14.txt"
printf 'the one before\n' >"$s/live/console-7.txt"
cat >"$s/live/running.json" <<EOF
{"cycles":[{"issue":12,"running":true,"status":"running","step":"develop","statusText":"writing code","startedAt":$((now - 600)),"updatedAt":$now,"console":"console-12.txt"},
           {"issue":14,"running":true,"status":"running","step":"plan","statusText":"planning","startedAt":$((now - 60)),"updatedAt":$now,"console":"console-14.txt"}],
 "last":{"issue":7,"running":false,"status":"aborted","step":"review","statusText":"","startedAt":$((now - 7200)),"updatedAt":$((now - 3600)),"console":"console-7.txt"}}
EOF
echo '{"generatedAt":"2026-10-10T00:00:00Z","paused":false,"running":[{"number":12,"title":"First"},{"number":14,"title":"Second"}],"queued":[],"awaiting":[],"stopped":[],"completed":[],"links":{"all":"#","queued":"#","awaiting":"#","stopped":"#","completed":"#"}}' >"$s/live/status.json"
(cd "$s" && python3 -m http.server 8765 >/dev/null 2>&1 &)
```

Open `http://127.0.0.1:8765/fun/index.html#console` in the headless Chrome (Playwright) and check, at a wide and at a phone width:
- three tabs: `#12`, `#14`, `#7 ENDED`; `#12` is selected and its console text is shown,
- clicking `#14` shows `second cycle` and the title of issue 14; the choice survives the ten-second refresh,
- clicking `#7 ENDED` shows the tag ENDED while the door still says `WORKING ON 2 ISSUES`,
- with focus on a tab, the right arrow moves to the next tab and selects it,
- with `"cycles":[]` and only `last` in `running.json`, the row is hidden.

Stop the server and remove the directory afterwards: `pkill -f 'http.server 8765'; rm -rf "$s"`.

The plain dashboard is a template of the docs server and cannot be opened this way. Read its code against the factory floor's once more instead; its logic is the tested `view()`.

- [ ] **Step 6: `DESIGN.md` and the manual**

`DESIGN.md`, under `### Production Console`, add a paragraph:

```markdown
When more than one cycle can be shown, a row of tabs stands above the screen:
one per running cycle, named by its issue number, and one for the cycle that
finished last, marked ENDED. A tab is a Bone button with the 4px outline; the
selected one is Brass. The tag in the panel head describes the selected cycle.
The door and the lamp say LIVE while any cycle runs. The plain dashboard has
the same row.
```

`host/docs/operating.md`: after the sentence `When the cycle starts and when it ends, the bot comments on the issue. The token usage per agent role is posted on the pull request.` add a paragraph:

```markdown
The console on the [factory floor](/) and on the [plain dashboard](/dashboard)
shows one cycle at a time. When several are running, or one is running and an
earlier one can still be read, a row of tabs above it names each by its issue
number.
```

`host/docs/features.md`: add a row after the `Production console` row, in that table's column widths:

```markdown
| Console tabs               | A tab per running cycle and one for the cycle that finished last, on the factory floor and the plain dashboard                         | Tested without a browser, and seen with fake files; not yet seen on a host |
```

- [ ] **Step 7: Gate and commit**

Run: `scripts/verify.sh --full`
Expected: every line `ok` (or `skip` where a tool is missing), with `the front page's tests` among them.

```bash
git add host/fun/console-tabs.js test/console_tabs_test.mjs host/fun/game-view.js host/fun/index.html host/fun/game-view.css \
  host/docs/dashboard.frag host/fun/manual.css DESIGN.md host/docs/operating.md host/docs/features.md
git commit -m "The console has a tab per cycle"
```

---

### Task 6: Slots

**Files:**
- Modify: `libexec/dispatch`
- Modify: `host/docs/host.md`, `host/docs/operating.md`, `host/docs/tools.md`, `host/docs/features.md`, `host/docs/llms.txt`, `host/docs/pi-software-factory.md`, `README.md`
- Test: `test/dispatch_test.sh`

**Interfaces:**
- Consumes: `TEHAS_TICKET_CONCURRENCY` (Task 1); `tehas_cycles_running`, `running`, `want`, `picked`, `by`, `start_cycle` in `libexec/dispatch` (Task 2).

- [ ] **Step 1: Write the failing tests**

In `test/dispatch_test.sh`, add after the `release()` helper:

```bash
# An instance that works on <n> issues side by side. The host's CPU count is
# read with nproc; a stub gives every test machine room for the split.
t_stub nproc 'echo 8'
slots() { t_instance "${T_MINIMAL[@]}" "TEHAS_TICKET_CONCURRENCY=$1" "TEHAS_WORK_CPUS=$1" "${@:2}"; }
ready() { # issue... : the queue, each labelled by alice; "8!" is urgent
  local n list=""
  for n in "$@"; do
    list="$list,{\"number\":${n%!},\"labels\":[{\"name\":\"tehas:ready\"}$([ "$n" = "${n%!}" ] || echo ',{"name":"tehas:urgent"}')]}"
    cp "$fx/events-7.json" "$fx/events-${n%!}.json" 2>/dev/null || true
  done
  echo "[${list#,}]" >"$fx/ready.json"
}
```

In `reset()`, add `"$fx"/events-*` to the first `rm -f`. (`reset` writes `events-7.json` again a few lines later.)

Add these cases just before the comment `# Arguments. A poll starts only from an argument this command knows`:

```bash
t_case "two slots: two issues are started in one poll, in order"
reset
slots 2
ready 7 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"
t_contains "$(t_calls gh)" "issue edit 8 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "the checkout is fast-forwarded and the index refreshed once, before the first start"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(find "$T/calls/index.late" 2>/dev/null | wc -l)" 0

t_case "one slot by default: the second issue waits"
reset
ready 7 8
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"

t_case "an urgent issue is started first"
reset
slots 2
ready 7 8! 9
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 7 "

t_case "a running cycle takes a slot"
reset
slots 2
ready 7 8
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "with every slot taken nothing is started, and no issue is looked at"
reset
slots 2
ready 7 8
hold 9
hold 10
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue list"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 2 cycles are running (limit 2)"
release

t_case "the index is not refreshed under a running cycle"
reset
slots 2
ready 7
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(t_calls fake-index)" ""
t_contains "$t_out" "the code index stays as it is: a cycle is running"
release

t_case "running cycles count towards the cap on unreviewed work"
reset
slots 3 TEHAS_MAX_AWAITING=3
ready 7 8
echo '[{"number":5}]' >"$fx/awaiting.json"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
release

t_case "at the cap nothing is started, whatever slots are free"
reset
slots 3 TEHAS_MAX_AWAITING=2
ready 7 8
echo '[{"number":5}]' >"$fx/awaiting.json"
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 issues await merge and 1 cycles are running (limit 2)"
release

t_case "an issue that is not approved does not use a slot"
reset
slots 2
ready 7 8 9
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "8 9 "
t_lacks "$(t_calls gh)" "issue edit 7 "

t_case "a cycle that does not start does not stop the next one, and the poll ends with 1"
reset
slots 2
ready 7 8
# fake-start fails for issue 7 only.
t_stub fake-start '[ "$1" != 7 ] || exit 1
: >"$TEHAS_HOME/state/cycles/$1.taken"'
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --remove-label tehas:running --add-label needs-human-attention"
t_lacks "$(t_calls gh)" "issue edit 8 --repo acme/widget --remove-label"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_stub fake-start '[ ! -e "$T/fx/start-fails" ] || exit 1
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/start.fd8"
[ -e "$T/fx/start-silent" ] || : >"$TEHAS_HOME/state/cycles/$1.taken"'

t_case "a dry run names every issue it would start and how much room there is"
reset
slots 3
ready 7 8
hold 9
t_run "$dispatch --dry-run"
t_contains "$t_out" "1 cycles running, room for 2 more"
t_contains "$t_out" "would run a cycle for #7 (labelled by alice)"
t_contains "$t_out" "would run a cycle for #8 (labelled by alice)"
t_eq "$(t_calls fake-start)" ""
release
```

Replace the Task 2 case `while a cycle is running nothing is started` so that its dry-run line reads:

```bash
t_contains "$t_out" "holding: 1 cycles are running (limit 1)"
```

Run: `bash test/dispatch_test.sh`
Expected: FAIL on the new cases (`expected '7 8 ', got '7 '` and the like).

- [ ] **Step 2: Implement in `libexec/dispatch`**

Replace the block Task 2 added after the pause switch (from `# 2. One cycle at a time.` to `want=1`) with:

```bash
# 2. The slots. A cycle that died holds no lock and is not counted.
running=$(tehas_cycles_running | tr '\n' ' ')
running=${running% }
count=$(wc -w <<<"$running")
count=$((count))
free=$((TEHAS_TICKET_CONCURRENCY - count))
if [ "$free" -le 0 ]; then
  [ "$dry" = 1 ] && log "holding: $count cycles are running (limit $TEHAS_TICKET_CONCURRENCY)"
  exit 0
fi
```

Replace the awaiting-merge block (from `# 3. Do not pile up unreviewed work.` to its `fi`) with:

```bash
# 3. Do not pile up unreviewed work. A running cycle is a pull request on its
# way, so it counts: otherwise the slots could overshoot the cap.
gh_json issue list --repo "$repo" --state open --label tehas:awaiting-merge --limit 100 --json number
awaiting=$(jq length <<<"$json") || { log "ERROR: unreadable list of issues awaiting merge"; exit 1; }
room=$((max_awaiting - awaiting - count))
if [ "$room" -le 0 ]; then
  if [ "$dry" = 1 ]; then
    if [ "$count" = 0 ]; then
      log "holding: $awaiting issues await merge (limit $max_awaiting)"
    else
      log "holding: $awaiting issues await merge and $count cycles are running (limit $max_awaiting)"
    fi
  fi
  exit 0
fi
want=$((free < room ? free : room))
```

In the dry-run branch after the loop, add a first line:

```bash
if [ "$dry" = 1 ]; then
  log "$count cycles running, room for $want more"
  for n in "${picked[@]}"; do log "would run a cycle for #$n (labelled by ${by[$n]})"; done
  exit 0
fi
```

Replace the index refresh (the `index_log=` line and the call) with:

```bash
# The agents search an index of the tree the cycle is about to be cloned from.
# It is written only while no cycle reads it. A cycle without a fresh index
# still works, so a failure here is a warning.
if [ "$count" = 0 ]; then
  index_log=$TEHAS_HOME/logs/dispatch-work-${picked[0]}.log
  "$index_cmd" --before-cycle >>"$index_log" 2>&1 ||
    log "WARN: the code index was not refreshed, see $index_log"
else
  log "the code index stays as it is: a cycle is running on #${running// /, #}"
fi
```

In the header, replace the first paragraph and the rules:

```bash
# Deterministic, no model. It picks the open issues labelled tehas:ready that
# there is room for, marks each tehas:running and starts its cycle as
# tehas-cycle@<issue>.service. It does not wait for a cycle: tehasbot cycle
# runs it and reports the outcome. A systemd timer runs a poll every two
# minutes, whatever the cycles are doing.
```

```bash
# Rules, in order:
#   1. Label tehas:pause on the control issue stops all dispatch.
#   2. At most TEHAS_TICKET_CONCURRENCY cycles run at once.
#   3. Running cycles and tehas PRs that wait for a human together stay below
#      TEHAS_MAX_AWAITING.
#   4. An issue is eligible only if its tehas:ready label was applied by an
#      account in TEHAS_ALLOWED_LABELERS. The bot is never in that list.
#   5. tehas:urgent first, then the lowest issue number. A running cycle is
#      never stopped to make room.
```

and add to the settings list: `#   TEHAS_TICKET_CONCURRENCY  cycles side by side [1]`.

Run: `bash test/dispatch_test.sh`
Expected: `dispatch_test.sh: N cases, 0 failed`.

- [ ] **Step 3: The manual**

`host/docs/host.md`, section `## Limits and concurrency`. Replace the first paragraph with:

```markdown
`TEHAS_WORK_CPUS` and `TEHAS_WORK_MEMORY` bound all the cycles of a host
together. CPUs is a time quota (`docker --cpus`), not pinning. Memory is a hard
limit (`docker --memory`); a process that exceeds it is killed. The defaults are
the host's CPUs minus one and half its memory, which on the example host is 7
CPUs and about 7.5 GB. A value above what the host has is rejected.

`TEHAS_TICKET_CONCURRENCY` sets how many issues are worked on side by side,
each in a cycle, a container and a workspace of its own. It defaults to 1. Each
container gets the two limits above divided by it, whether or not the other
slots are in use. A share below one CPU is rejected, so the setting cannot
exceed the CPUs given.
```

Replace the paragraph that begins `It is a limit inside one cycle, not across cycles:` with:

```markdown
It is a limit inside one cycle. With `TEHAS_TICKET_CONCURRENCY` above 1, up to
that many cycles run at once and each has its own `TEHAS_CONCURRENCY`
workstreams, so the two multiply: 2 tickets and 4 workstreams are up to 8
developer agents calling the model backends. Nothing checks that against what
the backends can serve. Lower either one when the backends have few slots, when
the host is small, or if cycles start timing out.
```

Replace the last list item (`- one issue is worked on at a time. Running several issues at once is not supported: cycles share one checkout.`) with:

```markdown
- how many issues are worked on at once. That is `TEHAS_TICKET_CONCURRENCY`.
```

Add after that list:

```markdown
Cycles that run side by side know nothing of each other:

- Two issues that touch the same files give two pull requests that conflict.
  Label such issues one after the other.
- The code index is refreshed only when no cycle is running. On a host that
  is never idle it falls behind the branch; the dispatcher's log says so each
  time.
- `~/.vipune`, Pi's `sessions` and the two caches are shared between cycles.
```

`host/docs/operating.md`:
- Replace `The dispatcher takes one issue at a time, urgent first, then the lowest issue number.` with `The dispatcher takes issues urgent first, then by the lowest issue number: one at a time unless the host is set to work on more (see [The host](/host)). A running cycle is never stopped to make room for an urgent one.`
- In `## Limits that hold work back`, replace `- One cycle at a time.` with:

```markdown
- `TEHAS_TICKET_CONCURRENCY` cycles at a time: 1 by default.
```

  and replace the `TEHAS_MAX_AWAITING` item with:

```markdown
- No new cycle while 3 issues carry `tehas:awaiting-merge` or are being worked
  on (`TEHAS_MAX_AWAITING`). Unreviewed pull requests are the queue you control.
```

`host/docs/tools.md`, `## tehas dispatch`:
- replace `tehas dispatch                 # start at most one cycle, then return` with `tehas dispatch                 # start the cycles there is room for, then return`,
- replace the sentence `Order of rules: pause label on the control issue, the cap on issues awaiting merge, who applied `tehas:ready`, then urgent first and lowest number.` with `Order of rules: pause label on the control issue, the free slots (`TEHAS_TICKET_CONCURRENCY` less the running cycles), the cap on issues awaiting merge or being worked on, who applied `tehas:ready`, then urgent first and lowest number.`
- in the paragraph Task 2 wrote, replace `then refreshes the code index` with `then, if no cycle is running, refreshes the code index`, and `It then marks the issue` with `It then marks each issue`.
- in the `TEHAS_MAX_AWAITING` row of the settings table, replace the meaning with `Open `tehas:awaiting-merge` issues, plus running cycles, allowed before holding`.

`host/docs/features.md`:
- replace the row `| One cycle at a time | ...` with (keeping the table's column widths):

```markdown
| Ticket concurrency                      | `TEHAS_TICKET_CONCURRENCY` cycles side by side, 1 by default, each in its own unit, container and workspace. Urgent first (`tehas:urgent`), then lowest issue number | One at a time: verified before the dispatcher was split into a poll and a cycle unit; the split is covered by the shell tests, not yet seen on a host. Above 1: not yet observed |
```

- in `## Known limits`, replace `- One host, one project, one cycle at a time.` with `- One host, one project. Cycles that run side by side do not know of each other: two issues that touch the same files give conflicting pull requests.`

`host/docs/llms.txt`: replace `- One instance is one host serving one product repository, one cycle at a time.` with `- One instance is one host serving one product repository, one cycle at a time by default and more if set.`

`host/docs/pi-software-factory.md`: replace `Issues wait in GitHub and are picked up one at a time, urgent ones first` with `Issues wait in GitHub and are picked up as slots come free, urgent ones first`, keeping the table's column width.

`README.md`: replace `2. A dispatcher polls the queue every two minutes and starts one cycle at a time.` (two lines) with:

```markdown
2. A dispatcher polls the queue every two minutes and starts a cycle for each
   issue there is room for: one at a time by default.
```

- [ ] **Step 4: Gate and commit**

Run: `scripts/verify.sh --full`
Expected: every line `ok` (or `skip` where a tool is missing).

```bash
git add libexec/dispatch test/dispatch_test.sh host/docs/host.md host/docs/operating.md host/docs/tools.md \
  host/docs/features.md host/docs/llms.txt host/docs/pi-software-factory.md README.md
git commit -m "The dispatcher starts up to TEHAS_TICKET_CONCURRENCY cycles"
```

---

## The pull request

Open one pull request from `ticket-concurrency` and stop. Its text, plain and short:

- What it does: the setting, the poll and the cycle unit, a workspace and transcripts per issue, the live view, the console's tabs.
- **Security change (hard rule 4):** the paragraph from Task 3's commit body.
- What is not proven: nothing here has run on a host. The default of 1 is covered by the shell tests. A value above 1 has not been observed.
- What a person must do on a host: pull, run `install.sh`, and apply the `units` step it names (the new `tehas-cycle@.service` needs systemd's reload). An upgrade during a cycle is safe: the old dispatcher holds its lock to the end of that cycle.
- Left on a host by the upgrade: the old single workspace `~/tehas/work/<name>`. It can be deleted by hand.
- Proposed for `instances/tehas-tehas/`, not made: nothing. The setting stays at its default until someone chooses a value.
- The gate's output from the last task, quoted.
