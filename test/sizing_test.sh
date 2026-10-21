#!/usr/bin/env bash
# shellcheck shell=bash
# Tests for lib/sizing.sh: tehas_issue_size, the four counts an issue body
# gives itself. Bodies are written inline, and passed to the measure in the
# environment so a tick in one is never run by the shell that carries it.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

sizing=$T_ROOT/lib/sizing.sh

# size <body>: what tehas_issue_size prints for that body on its stdin.
size() {
  t_out=$(env -i PATH="$PATH" BODY="$1" bash -c \
    ". '$sizing'; tehas_issue_size <<<\"\$BODY\"" 2>&1)
  t_rc=$?
}

t_case "a narrow issue: two files, one of them a test"
size 'Give `lib/sizing.sh` one function.

## The change

1. `lib/sizing.sh` holds tehas_issue_size.
2. the gate lints it.

## Done when

- `test/sizing_test.sh` has a case for it.
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":2,"tests":1,"deliverables":2,"done_when":1}'

t_case "a wide issue: six files, two tests, four deliverables"
size 'Size an issue from its body.

## What to change

1. `lib/sizing.sh` with tehas_issue_size.
2. `test/sizing_test.sh` with the cases.
3. `docs/specs/sizing.md` says the rules, and the manual says so too.
4. `console/src/size.rs` is for a later issue.

Not `test/work_test.sh`.

## Done when

- `test/sizing_test.sh` runs.
- `test/limit_test.sh` still passes.
- `scripts/verify.sh` passes, and the gate lints it.
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":6,"tests":2,"deliverables":4,"done_when":3}'

t_case "a path named only on a Not, Do not or Leave line is not counted"
size 'Touch `lib/a.sh`.

Not `test/work_test.sh`
Do not touch `lib/b.sh`
Leave `instances/example/instance.env` alone

## The change

1. one file.
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":1,"tests":0,"deliverables":1,"done_when":0}'

t_case "a path refused on one line and asked for on another counts once"
size 'Not `test/work_test.sh`

## The change

1. `test/work_test.sh` stays as it is and `lib/a.sh` changes.
2. `lib/a.sh` is the only file written.
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":2,"tests":1,"deliverables":2,"done_when":0}'

t_case "duplicates count once, whatever the line they are on"
size '`lib/a.sh`, `lib/a.sh` and again `lib/a.sh`.

`bin/tehas` too, and `bin/tehas`.
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":2,"tests":0,"deliverables":0,"done_when":0}'

t_case "an empty body is four zeros, and so is no body at all"
size ''
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":0,"tests":0,"deliverables":0,"done_when":0}'
t_run ". '$sizing'; tehas_issue_size </dev/null"
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":0,"tests":0,"deliverables":0,"done_when":0}'

t_case "what is not a path: no slash and no known extension, or no backticks"
size 'Run tehas work on notes.txt, and read README without the extension.

`tehas work` `notes.txt` `a.SH` `libexec/work`
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":1,"tests":0,"deliverables":0,"done_when":0}'

t_case "every extension the measure knows counts without a slash"
size '`a.sh` `b.rs` `c.md` `d.json` `e.toml` `f.yml` `g.txt` `h.py`
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":6,"tests":0,"deliverables":0,"done_when":0}'

t_case "a test/ path is one that starts with test/"
size '`test/a.sh` `a/test/b.sh` `lib/test/c.sh`
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":3,"tests":1,"deliverables":0,"done_when":0}'

t_case "deliverables are counted only under a heading that names the change"
size '## Files

1. `lib/a.sh`
2. `lib/b.sh`

## The change

1. one
2. two

## Notes

3. three
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":2,"tests":0,"deliverables":2,"done_when":0}'

t_case "the headings the measure answers to, and the ones it does not"
for heading in "The change" "the Change" "Deliverables" "deliverable" "What to change" "WHAT TO CHANGE"; do
  size "## $heading

1. one
2. two
"
  [ "$t_out" = '{"files":0,"tests":0,"deliverables":2,"done_when":0}' ] ||
    t_bad "heading '$heading' gave $t_out"
done
for heading in "Changes to make" "The changes" "Deliverable list" "How to change it" "Files, and only these" "Done when possible"; do
  size "## $heading

1. one
2. two

- a
"
  [ "$t_out" = '{"files":0,"tests":0,"deliverables":0,"done_when":0}' ] ||
    t_bad "heading '$heading' gave $t_out"
done

t_case "a Done when section counts its bullets and ends at the next heading"
size '## Done when

- one
- two
- three

## Notes

- four
- five
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":0,"tests":0,"deliverables":0,"done_when":3}'

t_case "a numbered list outside a change heading and bullets outside Done when count for nothing"
size 'Intro

- a
- b

1. one
2. two

### Done when

- c
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":0,"tests":0,"deliverables":0,"done_when":1}'

t_case "the heading of another section ends a Done when section"
size '# Top

## Done when

- one

## The change

1. two
'
t_eq "$t_rc" 0
t_eq "$t_out" '{"files":0,"tests":0,"deliverables":1,"done_when":1}'

t_case "the measure reads only its stdin: nothing is opened and nothing is written"
before=$(find "$T" -printf '%p\n' | LC_ALL=C sort)
t_run ". '$sizing'; printf '%s' 'x' | tehas_issue_size >/dev/null"
t_eq "$t_rc" 0
after=$(find "$T" -printf '%p\n' | LC_ALL=C sort)
t_eq "$after" "$before"

t_case "sourcing it prints nothing, needs no setting, and takes no argument"
t_run "env -i PATH=\$PATH HOME=\$HOME bash -c \". '$sizing'\""
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "the answer is one line of four whole numbers in the order named"
size '## The change

1. `lib/a.sh`

## Done when

- `test/a_test.sh`
'
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | wc -l | tr -d ' ')" 1
t_eq "$(jq -r 'keys_unsorted | join(",")' <<<"$t_out")" "files,tests,deliverables,done_when"
t_eq "$(jq -r '[.files, .tests, .deliverables, .done_when] | map(type) | join(",")' <<<"$t_out")" \
  "number,number,number,number"
t_eq "$(jq -r '[.files, .tests, .deliverables, .done_when] | all(. >= 0 and (. | floor) == .)' <<<"$t_out")" true

t_done
