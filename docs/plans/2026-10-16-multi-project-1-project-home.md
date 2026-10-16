# Multi-project, step 1: project home, no behaviour change. Implementation plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Every path that belongs to one project goes through `TEHAS_PROJECT_HOME`, and every host-global name (units, containers, the Pi agent directory) goes through one helper. Both equal today's values, so nothing a host does changes.

**Architecture:** `lib/config.sh` derives `TEHAS_PROJECT_HOME` (equal to `TEHAS_HOME` for now) and gains `tehas_name` and `tehas_agent_dir`. Scripts swap `$TEHAS_HOME/state` and the like for `$TEHAS_PROJECT_HOME/state`, except a short list of host-level files. A new test, `test/layout_test.sh`, fails when installed code names a per-project path under `$TEHAS_HOME` again. Step 2 of the spec then only has to change what the derivations return.

**Tech Stack:** bash, jq, shellcheck; the shell test harness in `test/lib.sh`.

**Spec:** `docs/specs/2026-10-16-multi-project.md`, sections "Layout on the host", "Names that are global on the host" and "Order of work" step 1.

## Global Constraints

- No behaviour change. Every existing test passes unchanged. Not one existing assertion is edited, and `test/work_test.sh`, `plan_test.sh` and `dev_test.sh` (the rule 4 pins) are not touched.
- AGENTS.md rule 3: `lib/config.sh` stays free of side effects. `tehas_load` never exits. The new helpers print and return, and write nothing.
- AGENTS.md style: `set -uo pipefail` as the script already has it, two spaces, `shellcheck -x` clean, no blanket disables. Comments are terse and say why.
- Per-project subdirectories: `state/`, `live/`, `logs/`, `work/`, `repos/<product>`, `dev/`, `protolab/`, `preview/`, `tmp/improve/`.
- Host-level, staying under `$TEHAS_HOME`:
  - `state/update.lock`, `state/update-hold`, `state/update-pending`, `state/edge-takeover`, `state/platform-fetch`, `state/improve.lock`, `state/dispatch.again`
  - `live/top.json`, `live/energy.json`, `live/platform.json`
  - `repos/pi-rukas`
  - `tmp/` as scratch (`TMPDIR`)
  - everything `bootstrap.sh` touches
- `mcp.json` stays at `$HOME/.pi/agent/mcp.json`. Only `models.json`, `settings.json` and `ensemble-runs/` move to `tehas_agent_dir`.
- Edge-name parsing (the regexes in `libexec/preview` at 987, 999 and 1031, and the jq at `preview:695` and `protolab:111`) is left alone. That is step 4.
- Commit after each task. Run the affected test files while iterating, and `scripts/verify.sh` once at the end. Write its output to a file and check both its exit status and its `FAIL` lines; never pipe it.

## Review Focus

1. **A file read in one script and written in another, where only one side was converted.** Both sides resolve to the same path today, so no test catches it. Step 2 would split them apart. Example: `live/cycles/<n>.json`, written by `work` and read by `dispatch:923`. The layout test catches this for code. Tasks 3 and 4 also grep for every file name they touch, to find the readers.
2. **`TEHAS_PROJECT_HOME` unset in a script that runs before `tehas_load`, or that never calls it.** The path then becomes `/state/...` and the script writes at the root. Task 1 sets the variable before the first `return` in `tehas_load`, and its test runs with a broken `instance.env` to show it is still set.
3. **`mcp.json` mounted from the agent directory by mistake.** It must stay at `$HOME/.pi/agent/mcp.json`. The `work_test.sh` pins catch this today because both paths are equal. Task 1's test of `tehas_agent_dir` and the comment at the mount in Task 2 cover it for step 2.
4. **The default of `TEHAS_WORK_REPO`.** It moves under `TEHAS_PROJECT_HOME`. An instance that sets `TEHAS_WORK_REPO` itself keeps its own value. Task 1 tests both.
5. **The host-level allowlist growing to hide a per-project file.** The layout test lists the allowed names exactly. Its last case asserts that each entry is still used somewhere, so a dead entry fails.

---

### Task 1: The derivations and the names helper

**Files:**
- Modify: `lib/config.sh`. Set `TEHAS_PROJECT_HOME` at the top of `tehas_load` (around line 206). Point the default of `TEHAS_WORK_REPO` (252) and `TEHAS_WORK_DIR` (256) at it. Add two helpers next to `tehas_edge_name` (around 438), and list them in the header comment.
- Test: `test/config_test.sh`, new cases at the end, before `t_done`.

**Interfaces:**
- Produces:
  - `TEHAS_PROJECT_HOME`: exported by `tehas_load`, equal to `$TEHAS_HOME`, set before any `return 78`. Also set by `host/env.sh` right after `TEHAS_HOME`, for code that runs before `tehas_load`.
  - `tehas_name <kind> [<arg>]`: prints one name and returns 0. Returns 1 for an unknown kind or a missing argument. The kinds and what they print today:
    - `cycle-unit N` prints `tehas-cycle@N.service`
    - `work N` prints `tehas-work-N`
    - `plan N` prints `tehas-plan-N`
    - `dev NAME` prints `tehas-dev-NAME`
    - `index` prints `tehas-index`
    - `improve` prints `tehas-improve`
    - `preview N` prints `tehas-preview-N`
    - `preview-image N` prints `tehas-preview:N`
  - `tehas_agent_dir`: prints `$HOME/.pi/agent`, the directory that holds this project's `models.json`, `settings.json` and `ensemble-runs/`.

- [ ] **Step 1: Write the failing tests**

```bash
t_case "the project home is the tehas home, and is set even when the instance does not load"
t_instance "${T_MINIMAL[@]}"
t_run "$load && echo \"\$TEHAS_PROJECT_HOME|\$TEHAS_WORK_REPO|\$TEHAS_WORK_DIR\""
t_eq "$t_out" "$T/home/tehas|$T/home/tehas/repos/widget|$T/home/tehas/work/widget"
t_instance "${T_MINIMAL[@]}" TEHAS_WORK_REPO=/elsewhere/widget
t_run "$load && echo \"\$TEHAS_WORK_REPO\""
t_eq "$t_out" "/elsewhere/widget"
t_instance "TEHAS_REPO=not-a-repo"
t_run "$load; echo \"rc=\$? \$TEHAS_PROJECT_HOME\""
t_eq "$t_out" "rc=78 $T/home/tehas"
t_instance "${T_MINIMAL[@]}"

t_case "every host-global name comes from one helper"
t_run "$load && for k in 'cycle-unit 12' 'work 12' 'plan 12' 'dev alice' index improve 'preview 12' 'preview-image 12'; do tehas_name \$k; done"
t_eq "$t_out" "tehas-cycle@12.service
tehas-work-12
tehas-plan-12
tehas-dev-alice
tehas-index
tehas-improve
tehas-preview-12
tehas-preview:12"

t_case "the names helper refuses an unknown kind and a missing argument"
t_run "$load && { tehas_name bogus 1; echo rc=\$?; tehas_name work; echo rc=\$?; }"
t_eq "$t_out" "rc=1
rc=1"

t_case "the agent directory is Pi's own"
t_run "$load && tehas_agent_dir"
t_eq "$t_out" "$T/home/.pi/agent"
```

If `TEHAS_REPO=not-a-repo` is not the cheapest way to make `tehas_load` return 78 after its first line, use any other malformed setting that `config_test.sh` already uses for a 78.

- [ ] **Step 2: Run the tests and check that they fail**

Run: `bash test/config_test.sh`
Expected: FAIL on the four new cases (empty `TEHAS_PROJECT_HOME`, `tehas_name: command not found`).

- [ ] **Step 3: Implement it in `lib/config.sh` and `host/env.sh`**

`tehas_name` is a `case` on the kind. Kinds that take an argument return 1 when it is empty.

- [ ] **Step 4: Run the tests and check that they pass**

Run: `bash test/config_test.sh && bash test/generic_test.sh`
Expected: both end with their tally and exit 0.

- [ ] **Step 5: Commit**

```bash
git add lib/config.sh host/env.sh test/config_test.sh
git commit -m "The project home and one helper for host-global names"
```

### Task 2: The layout test, and the libraries

**Files:**
- Create: `test/layout_test.sh`
- Modify:
  - `lib/cycles.sh` (36, 41)
  - `lib/edge.sh` (27, 28)
  - `lib/improve.sh` (43, 64, 77, 82, 110, 118)
  - `lib/live.sh` (28, 38)
  - `lib/planning.sh` (245)
  - `lib/sandbox.sh` (40, 94-95, 99-113, 120)
  - `lib/top.sh` (113)
  - `lib/describe.sh` (68)

**Interfaces:**
- Consumes: `TEHAS_PROJECT_HOME`, `tehas_agent_dir` (Task 1).
- Produces: `test/layout_test.sh` with an array `pending=(...)` of files not yet converted. Tasks 3 and 4 remove their files from it, and Task 4 removes the array.

- [ ] **Step 1: Write the layout test**

Model it on `test/internals_test.sh`: the same `code()` list of installed files, minus `bootstrap.sh`, `console/src` and `scripts`. Skip comment lines (first non-blank character `#`) and the files in `pending`.

```bash
# Per-project paths named under TEHAS_HOME, as extended regular expressions.
per_project='TEHAS_HOME\}?/(state|live|logs|work|dev|protolab|preview)(/|"|$)|TEHAS_HOME\}?/repos/[^p]|TEHAS_HOME\}?/repos/\$|TEHAS_HOME\}?/tmp/improve'
# The host-level files that stay under TEHAS_HOME.
host_level='TEHAS_HOME\}?/state/(update\.lock|update-hold|update-pending|edge-takeover|platform-fetch|improve\.lock|dispatch\.again)|TEHAS_HOME\}?/live/(top|energy|platform)\.json'
```

Cases:
- `"installed code names no per-project path under TEHAS_HOME"`: the hits of `per_project` that do not match `host_level`, outside `pending`, must be empty. The failure message lists `file:line`.
- `"the Pi agent directory is named only through tehas_agent_dir"`: no installed code outside `lib/config.sh` names `.pi/agent/models.json`, `.pi/agent/settings.json` or `.pi/agent/ensemble-runs` with `$HOME`. Container-side paths `/home/vscode/.pi/agent/...` are allowed.
- `"each pattern finds what it is for"`: feed sample lines such as `x=$TEHAS_HOME/state/cycles/1.lock` (hit), `x=$TEHAS_HOME/repos/pi-rukas` (no hit), `x=$TEHAS_HOME/state/update.lock` (allowed) and `x=$TEHAS_PROJECT_HOME/state` (no hit).
- `"every host-level entry is still used"`: each alternative in `host_level` matches at least one line of installed code.

- [ ] **Step 2: Run it and check that it fails on the libraries**

Set `pending` to every file the Task 2 to 4 lists name, except the `lib/` files.
Run: `bash test/layout_test.sh`
Expected: FAIL. The listed lines are all in `lib/`.

- [ ] **Step 3: Convert the libraries**

In each of these files, replace `$TEHAS_HOME/` with `$TEHAS_PROJECT_HOME/` for per-project paths, including the paths written in comments.
- `lib/edge.sh`: locks go in the project's `state/`.
- `lib/improve.sh`: `tehas_improve_dir` prints `$TEHAS_PROJECT_HOME/tmp/improve`.
- `lib/sandbox.sh`:
  - `agent=$(tehas_agent_dir)` for `models.json` and `settings.json`, and in `tehas_sandbox_retry`.
  - `mcp.json` is mounted from `$HOME/.pi/agent/mcp.json`. Add a short comment there: it is generic and not per project.
  - `tehas_index_dir` prints `$TEHAS_PROJECT_HOME/state/index`.
- `lib/describe.sh:68`: read `"$(tehas_agent_dir)/models.json"`.

- [ ] **Step 4: Run the affected tests**

Run: `for t in layout cycle improve plan work dev sandbox describe top edge preview protolab questions; do bash test/${t}_test.sh >/dev/null || echo "FAIL $t"; done`
Expected: no `FAIL` line.

- [ ] **Step 5: Commit**

```bash
git add test/layout_test.sh lib/
git commit -m "Libraries name per-project paths through the project home, and a test keeps it so"
```

### Task 3: The cycle path: dispatch, cycle, work, plan, dev, index

**Files:**
- Modify:
  - `libexec/dispatch`: every line the Task 2 grep lists, plus models at 289, 293 and 346, and names at 717, 733, 793, 950-981, 1020-1029, 1063 and 1264
  - `libexec/cycle` (173, 175, 300, 537-559). Line 645, `dispatch.again`, stays host-level.
  - `libexec/work` (268-302, 535, 555, and the comment paths at 61, 119 and 180-191)
  - `libexec/plan` (168-170, 182, and the comments at 32 and 87-89)
  - `libexec/dev` (161, 257, 271, 288, and the comments at 9 and 45)
  - `libexec/index` (83-88, 134-140, 169, 182). Line 169's `tmp/` is scratch and stays.
  - `libexec/energy:64`: read `${TEHAS_MODELS_FILE:-$(tehas_agent_dir)/models.json}`. `live/energy.json` stays host-level.
  - `libexec/improve` (101, 105, 111, 225, 231, 235). Line 112, `improve.lock`, stays host-level.
  - `test/layout_test.sh`: remove these files from `pending`.

**Interfaces:**
- Consumes: `TEHAS_PROJECT_HOME`, `tehas_name`, `tehas_agent_dir` (Task 1).

- [ ] **Step 1: Remove the files from `pending` and check that the layout test fails**

Run: `bash test/layout_test.sh`
Expected: FAIL, listing lines in these files only.

- [ ] **Step 2: Convert them**

- Paths: as in Task 2.
- Names: every literal `tehas-cycle@$n.service`, `tehas-work-$n`, `tehas-plan-$n`, `tehas-dev-$name`, `tehas-index` and `tehas-improve` becomes `$(tehas_name <kind> <arg>)`, assigned once to a local where a function uses the name more than once. In the text of a comment posted on GitHub, `dispatch:793` keeps the name it prints. Take it from the helper as well.
- Transcripts: `runs=$(tehas_agent_dir)/ensemble-runs/issue-$issue` in `work`, and the same with `plan-$issue` in `plan`.
- For each state or live file name you change, run `git grep -n '<file name>' -- libexec lib` to find its readers in other scripts and convert them in the same commit. That is Review Focus 1.

- [ ] **Step 3: Run the affected tests**

Run: `for t in layout dispatch cycle work plan dev index energy improve questions headless rukas; do bash test/${t}_test.sh >/dev/null || echo "FAIL $t"; done`
Expected: no `FAIL` line.

- [ ] **Step 4: Commit**

```bash
git add libexec/dispatch libexec/cycle libexec/work libexec/plan libexec/dev libexec/index libexec/energy libexec/improve test/layout_test.sh
git commit -m "The cycle path names per-project paths and host-global names through config.sh"
```

### Task 4: The rest: status, health, pause, previews, Protolab, install

**Files:**
- Modify:
  - `libexec/status` (48, 118). Line 56, the platform stamp, stays host-level. So does `live/platform.json`, written at 85.
  - `libexec/health` (40, 102). It reads `platform.json` from `$TEHAS_HOME/live`.
  - `libexec/summary` (42)
  - `libexec/top` (63). It reads per-project files from the project's `live/`, and `top.json` from `$TEHAS_HOME/live`.
  - `libexec/top-publish` (45). It writes to `$TEHAS_HOME/live`, so it stays host-level. Only the comment needs a look.
  - `libexec/pause` (52), `libexec/resume` (50)
  - `libexec/preview` (99-102, 140). Names at 199, 204, 205, 575, 631-675 and 878 go through `tehas_name preview` and `preview-image`. Leave the regexes at 987, 999 and 1031.
  - `libexec/protolab` (76, 77)
  - `install.sh`:
    - 556: make the per-project directories under `$TEHAS_PROJECT_HOME` and `logs state tmp live` under `$TEHAS_HOME`.
    - 605: `index.json` in the project's `state/`.
    - 655: `dispatch.lock` in the project's `state/`.
    - 230 and 241-245: `models.json` and `settings.json` go to `$(tehas_agent_dir)`. `mcp.json` stays at `$HOME/.pi/agent`.
  - `libexec/update`: check each line against the host-level list. They should all stay host-level.
  - `test/layout_test.sh`: delete the `pending` array.

**Interfaces:**
- Consumes: `TEHAS_PROJECT_HOME`, `tehas_name`, `tehas_agent_dir` (Task 1).

- [ ] **Step 1: Delete `pending` and check that the layout test fails on these files only**

Run: `bash test/layout_test.sh`
Expected: FAIL, listing lines in the files above.

- [ ] **Step 2: Convert them, as in Task 3, grepping for every reader of each file name you change**

- [ ] **Step 3: Run the affected tests**

Run: `for t in layout status top pause preview protolab install update steps edge front_page; do bash test/${t}_test.sh >/dev/null || echo "FAIL $t"; done`
Expected: no `FAIL` line.

- [ ] **Step 4: Run the full gate once**

Run: `scripts/verify.sh >"$TMPDIR/verify.out" 2>&1; echo "exit $?"; grep -E '^(FAIL|ok|PASS)' "$TMPDIR/verify.out" | tail -20`
Expected: `exit 0` and no `FAIL` line. Quote the step lines in the handoff.

- [ ] **Step 5: Commit**

```bash
git add libexec/ install.sh test/layout_test.sh
git commit -m "Every per-project path and host-global name goes through config.sh"
```

## After the last task

Open a pull request against `main` that references the spec. Its text says three things:

- No behaviour change: the existing tests are unedited, and the rule 4 pins in `work_test.sh`, `plan_test.sh` and `dev_test.sh` are untouched.
- The new layout test keeps per-project paths out of `$TEHAS_HOME`.
- The manual in `host/docs/` does not change, because every path on a host is the same.
