# Auto-refine Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** A cycle that parks as `intent-park` with `underspecified` or `too-large` is refined once by the factory, and every refine session that files links the original and the filed issue both ways and closes or labels the original.

**Architecture:** `libexec/cycle` marks such a park (`refine: true` in `state/cycles/<n>.park`). A new rule in `libexec/dispatch`, right after rule 5 (new planning sessions), starts a `tehas:refine` session on a marked park when every check of the spec holds, on the cycle starter's approval. `lib/planning.sh` records `auto: true`, ends a refine session itself after the turn that files, and makes the links.

**Tech Stack:** bash, jq, gh; shell tests with `test/lib.sh` and the gh stub in `test/dispatch_lib.sh`.

**Spec:** `docs/specs/2026-10-24-auto-refine.md`

## Global Constraints

- `set -uo pipefail` style as the file has it, two spaces, `shellcheck -x` clean, no blanket disables.
- A refine starts only on the approval of an account in `TEHAS_ALLOWED_LABELERS` (`state/cycles/<n>.by`), on text unchanged since the parked cycle began.
- The factory starts at most one planning session per issue; never on an issue with `tehas:refined` or `tehas:replaced`, a plan history, a session record, or a body whose first line is `Replaces #<n>`.
- Park reasons that refine: `underspecified`, `too-large`, with cap `intent-park`. No other.
- Labels: `tehas:replaced` (original closed as not planned), `tehas:refined` (original kept open, later parts). `libexec/tidy` leaves both.
- Body line written on the filed issue: `Replaces #<n>` as its first line, then a blank line.
- Comment on the original: starts `Replaced by #<m>`.
- No change to `.pi/`, `AGENTS.md`, `scripts/verify.sh`, `scripts/check-generic.sh`, `instances/` other than `example/`, or the container's mounts.
- Prose: short sentences, no em dashes. The manual describes what is true; `features.md` marks it built and tested, not proven.

## Review Focus

- A poll that fails halfway through the links (a close or a comment fails) is repeated by the next one without saying `Replaced by` twice or adding the body line twice. Pinned in Task 4.
- A filed address in another repository gets no body edit and no sub-issue link. Pinned in Task 4.
- The issue text edited after the park: no refine, the mark is dropped. Pinned in Task 3.
- A dry run says what it would do and changes nothing: no label, no comment, no record, the mark kept. Pinned in Task 3.
- A paused factory keeps the mark and refines after resume, not before. Pinned in Task 3.

---

### Task 1: The cycle marks a park that may be refined

**Files:**
- Modify: `libexec/cycle` (the `park()` function, the `jq -n` that writes `$cycles/$issue.park`; the header comment that lists the park record)
- Test: `test/cycle_park_test.sh`

**Interfaces:**
- Produces: `state/cycles/<n>.park` gains `refine` (boolean): true exactly when `cap == "intent-park"` and `park_reason` is `underspecified` or `too-large`.

- [ ] **Step 1: Write the failing tests** in `test/cycle_park_test.sh`, using the file's existing way of making a result with a cap and a park reason:
  - `t_case "an intent park as underspecified is marked to be refined"`: `jq -r .refine "$state/7.park"` is `true`.
  - same for `too-large`.
  - `t_case "an intent park for another reason is not marked"`: `already-implemented` gives `false`.
  - `t_case "a step that failed is not marked"`: `step-failed:develop` with `parkReason` `underspecified` gives `false`.
- [ ] **Step 2: Run** `bash test/cycle_park_test.sh`. Expected: the new cases FAIL (`null` != `true`).
- [ ] **Step 3: Implement**: compute `refine` before the `jq -n` (`[ "$cap" = intent-park ]` and a `case` on `$park_reason`), pass it with `--argjson refine`, add `$refine` to the object.
- [ ] **Step 4: Run** `bash test/cycle_park_test.sh`. Expected: all PASS.
- [ ] **Step 5: Commit** `A cycle parked as underspecified or too large is marked for a refine`.

### Task 2: The planning record knows an automatic session, and hands one back

**Files:**
- Modify: `lib/planning.sh` (`plan_turn`, `plan_finish` (kept fields), `plan_history`, `plan_end`, `plan_lost`, the record comment at the top)
- Test: `test/plan_test.sh` or `test/dispatch_plan_test.sh`, whichever already tests `plan_end` at the turn limit and `plan_lost` (grep `reached its limit`)

**Interfaces:**
- Consumes: the caller sets `plan_auto=1` (global, default empty) with `who` and `plabel=tehas:refine` before `plan_turn <n> first <brief>`.
- Produces:
  - First-turn record gains `auto: true` when `plan_auto=1`; `plan_finish` keeps `auto` in `$kept` with `by`, `answers`, `label`; `plan_history` writes `auto: true` on the line when kept has it.
  - The start comment for an automatic session: `Tehas planning session started on \`<host>\` at <time> UTC to refine this issue: its cycle parked as \`<reason>\`, and @<who>'s \`tehas:ready\` authorizes one refine. Each turn ends in a comment here. Answer by ticking a box or by replying. To stop it, take \`tehas:question\` off while a question waits, or close the issue.` The reason comes from a global `plan_auto_reason`.
  - `plan_end <n> <text> [hand]`: with `hand`, the edit adds `needs-human-attention` instead of removing it. The turn-limit end in `plan_turn` and `plan_lost` pass `hand` when the record has `auto: true`. "Planning session ended by <login>" never does.
  - In auto mode `plan_turn` does not pass `--remove-label tehas:refine` (the issue does not carry it).

- [ ] **Step 1: Write the failing tests**:
  - an automatic first turn writes `auto: true` in the record and the history line; the start comment contains `its cycle parked as \`underspecified\``; the mark edit has no `--remove-label tehas:refine`.
  - an automatic session at `TEHAS_PLAN_MAX_TURNS` ends with `--add-label needs-human-attention` in the edit; a labelled one still has `--remove-label needs-human-attention`.
- [ ] **Step 2: Run** the test file. Expected: new cases FAIL.
- [ ] **Step 3: Implement** as in Interfaces.
- [ ] **Step 4: Run** the test file and `bash test/dispatch_plan_test.sh`. Expected: PASS.
- [ ] **Step 5: Commit** `An automatic planning session is on record and handed back to a person`.

### Task 3: The dispatcher refines a marked park once

**Files:**
- Modify: `libexec/dispatch`: a new rule "5a. Refining a parked cycle" right after the loop of rule 5 and before `if [ "$want" = 0 ]`; the header's rule list.
- Create: `test/dispatch_refine_test.sh`
- Modify: `test/dispatch_lib.sh` only if a fixture helper is needed (a `refine_park` helper may live in the new test file instead).

**Interfaces:**
- Consumes: Task 1's `refine` mark; Task 2's `plan_auto`, `plan_auto_reason`; existing `plan_state`, `park_id`, `control_ask`, `limit_plan_ok`, `slice_ok`, `mesh_ok`, `work_ok`, `unchanged_since`, `tehas_allowed_labeler`, `tehas_fixer_is`, `plan_turn`, `$plan_busy`.
- Produces: log lines `start refining #<n> (its cycle parked as <reason>, approved by <who>)`, `would start refining #<n> ...` on a dry run, `skip #<n>: not refined: <why>` when a check drops the mark.

For each `state/cycles/*.park` with `refine == true`, in issue order, one start per poll:
1. Wait (keep the mark, `break`) on: `$plan_busy`, `slice_ok`, `limit_plan_ok`; `mesh_ok || exit 0`.
2. `control_ask` on the control issue: drop the mark.
3. Read `gh issue view <n> --json state,labels,title,body`. Drop the mark, with the reason in the log, when: not OPEN; lacks `needs-human-attention` or `tehas:question`; has any of `tehas:ready tehas:running tehas:blocked tehas:plan tehas:refine tehas:refined tehas:replaced`; no `state/park-<n>.id`; `state/plan-<n>.json` or `state/cycles/plan-<n>.history.jsonl` exists; the body's first line matches `^Replaces #[0-9]+$`; `.by` is empty, the fixer, or not allowed; `unchanged_since` the park's `started` fails.
4. Dropping the mark rewrites the park file with `refine: false` (not on a dry run).
5. Start: write `$st.brief` exactly as rule 5 does for `tehas:refine` (share the code: move the brief writing into a function `plan_brief <n> <brief json> <refine file>` used by both), `rm -f "$(park_id "$n")"`, set `who`, `plabel=tehas:refine`, `plan_auto=1`, `plan_auto_reason`, clear the mark, `plan_turn "$n" first "$st.brief"`, `"$status_cmd"`, `exit 0`.

- [ ] **Step 1: Write the failing tests** in `test/dispatch_refine_test.sh` (source `lib.sh` and `dispatch_lib.sh`; a helper writes the park with `refine: true`, `started`, the `.by` file `alice`, the park id, the issue with `needs-human-attention` and `tehas:question`, and the edit fixture of an unchanged body):
  - starts a session: `plan_calls` has `--brief`, the brief ends with the refine brief and the "This request is issue #7 itself" line, the record has `by: alice`, `label: tehas:refine`, `auto: true`, the park id is gone, `refine` is false.
  - a second poll starts nothing (`plan_calls` unchanged).
  - each drop check, one case each: closed; `tehas:ready` on it; `tehas:refined` on it; no park id; a plan history line; body starts `Replaces #3`; `.by` is `mallory` (not allowed); body edited after `started`. Each: no `plan_calls`, `refine` false, log names the reason.
  - a sub-issue of a person's epic (no `Replaces` line) is refined.
  - paused (`pause_flag`): nothing starts, `refine` stays true; after the flag is removed the next poll starts it.
  - another planning turn running: waits, mark kept.
  - dry run: `would start refining #7`, no gh edit or comment, mark kept.
  - a park marked false, or a cap other than intent-park: nothing.
- [ ] **Step 2: Run** `bash test/dispatch_refine_test.sh`. Expected: FAIL.
- [ ] **Step 3: Implement** the rule and `plan_brief`.
- [ ] **Step 4: Run** `bash test/dispatch_refine_test.sh test/dispatch_plan_test.sh test/dispatch_park_test.sh` (each). Expected: PASS.
- [ ] **Step 5: Commit** `The dispatcher refines a cycle parked as underspecified or too large, once`.

### Task 4: A refine session that files links both issues and ends

**Files:**
- Modify: `lib/planning.sh`: `plan_post` and a new `plan_replace`.
- Modify: `test/dispatch_lib.sh`: gh stub answers for `issue view <m> --json body`, `issue edit <m> --body-file`, `api repos/acme/widget/issues/<m> --jq .id`, `api -X POST repos/acme/widget/issues/<n>/sub_issues`, `issue close`.
- Test: `test/dispatch_refine_test.sh` (labelled and automatic sessions both).

**Interfaces:**
- `plan_replace <n>`: called from `plan_post` in place of posting the question when the record's `label` is `tehas:refine` and `$st.out` has filed addresses. Returns 1 to have the next poll repeat it. Progress is kept in the record as `replace: {body: [m...], sub: [m...], said: true, done: true}` so a repeat skips what worked.
  1. For each filed address of `$repo`: edit the filed body to `Replaces #<n>\n\n<body>` unless its first line already is that; link it as a sub-issue (`gh api repos/$repo/issues/<m> --jq .id`, then `gh api -X POST repos/$repo/issues/<n>/sub_issues -F sub_issue_id=<id>`); a failed link is a WARN and counts as done.
  2. Once: `say` on the original `Replaced by #<m>[, #<m2>]. <the turn's text>`; addresses of other repositories are named as they are, with no link.
  3. Later parts: read from the filed body. The first filed issue of `$repo` whose body has a line matching `^#+ *Later parts` with a non-empty line after it that is not `None` (case-insensitive), or a body that cannot be read, means later parts.
  4. End: no later parts: `gh issue edit <n> --remove-label tehas:question --remove-label tehas:planning --add-label tehas:replaced`, then `gh issue close <n> --reason "not planned"`. Later parts: the same edit with `--add-label tehas:refined`, not closed. Then remove the record files as `plan_end` does.

- [ ] **Step 1: Write the failing tests**:
  - a labelled refine session whose turn filed `.../issues/12` with no later parts: edit of 12's body starts `Replaces #7`; a POST to `issues/7/sub_issues`; a comment on 7 starting `Replaced by #12`; an edit of 7 with `--add-label tehas:replaced`; `issue close 7 --repo acme/widget --reason not planned`; no question comment with boxes; the record is gone.
  - later parts in 12's body: `tehas:refined`, no close.
  - 12's body unreadable: `tehas:refined`, no close.
  - a body that already starts `Replaces #7`: no body edit.
  - the close fails on the first poll: the next poll closes it and the comment `Replaced by` was posted once.
  - a filed address in `other/repo`: no body edit, no sub-issue call for it, named in the comment.
  - a `tehas:plan` session that filed: unchanged (the question is posted as today).
  - an automatic session: the same as a labelled refine.
- [ ] **Step 2: Run** the test file. Expected: FAIL.
- [ ] **Step 3: Implement** `plan_replace` and the branch in `plan_post`.
- [ ] **Step 4: Run** `bash test/dispatch_refine_test.sh test/dispatch_plan_test.sh test/dispatch_plan_answer_test.sh test/plan_test.sh` (each). Expected: PASS.
- [ ] **Step 5: Commit** `A refine that files links the two issues and closes or labels the original`.

### Task 5: Labels, tidy, manual, changelog

**Files:**
- Modify: `libexec/labels`: `mk tehas:replaced` and `mk tehas:refined`, colour `c5def5`, descriptions `Tehas: replaced by the issue a refine filed` and `Tehas: refined; later parts are planned by hand`.
- Test: `test/labels_test.sh` (both made), `test/tidy_test.sh` (a closed issue keeps both).
- Modify: `host/docs/operating.md` (the label table, the park section, "Refining a request"), `host/docs/features.md` (new row "A cycle that cannot work an issue refines it", and the "Refining a request" row), `host/docs/troubleshooting.md` (the `underspecified` and `too-large` rows), `host/docs/tools.md` (`auto` on history lines, where the fields are listed), `CHANGELOG.md` under `## Unreleased` with an adopter action: run `tehas labels` once.

- [ ] **Step 1: Write the failing tests** in `test/labels_test.sh` and `test/tidy_test.sh`.
- [ ] **Step 2: Run** both. Expected: the labels case FAILS; the tidy case may already pass (tidy lists what it removes), which is fine: it pins the rule.
- [ ] **Step 3: Implement** the labels and write the manual pages and the changelog entry.
- [ ] **Step 4: Run** `scripts/verify.sh > "$scratch/verify.out" 2>&1; echo $?`, and check the exit status and that no line says FAIL.
- [ ] **Step 5: Commit** `Labels and manual for refining a parked cycle`.
