#!/usr/bin/env bash
# Tests for tehas plan, one turn of a planning session: later turns, the slice, and what a session carries.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/plan_lib.sh
. "$(dirname "$0")/plan_lib.sh" || exit 1

# --- Later turns and what the runner answers --------------------------------------

t_case "a later turn resumes the session and gives the answer as it was written, as a file in the workspace"
first_turn_ran
printf '%s\n' '- "Adopt all", but keep `tagged-at` open' '' "second line with 'quotes' and \$HOME" >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan --resume abc-123 --answer $ws/.tehas/answer.md "
t_eq "$(cat "$T/calls/answer.seen")" "$(cat "$T/answer.md")"
t_eq "$(cat "$T/calls/tree.status")" ""
t_eq "$([ -e "$ws/.tehas/brief.md" ] && echo present)" ""

t_case "a drafted plan is posted with its title and spec, its open points with the proposals, and one box; the draft is kept"
reset
shown
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .text "$T/out.json")" "Drafted plan: Faster search. It has 1 open point: answer them, ask for changes, or confirm to file it as it is.

### Faster search

## Context

Search is slow.

## Acceptance criteria

- p95 under 100 ms"
t_eq "$(jq -r .question.question "$T/out.json")" "Open points, with the planner's proposals:

1. Index on the title too?
   Proposed: Yes, both

Reply \`1\` (Update this issue) to make the plan, proposals included, this issue's text, or reply in your own words to change it."
t_eq "$(jq -c '[.question.options[].label]' "$T/out.json")" '["Update this issue"]'
t_eq "$(cat "$T/keep/exit")" 12
t_eq "$(jq -c . "$T/keep/draft.json")" '{"shown":{"body":"## Context\n\nSearch is slow.\n\n## Acceptance criteria\n\n- p95 under 100 ms","title":"Faster search"},"spec":"## Context\n\nSearch is slow.\n\n## Acceptance criteria\n\n- p95 under 100 ms","title":"Faster search"}'

t_case "a refine session's drafted plan also offers File as a new issue"
reset
shown
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md --kind refine"
t_eq "$t_rc" 0
t_contains "$(jq -r .question.question "$T/out.json")" "this issue's text, \`2\` (File as a new issue) to file it as its own issue, or reply in your own words to change it."
t_eq "$(jq -c '[.question.options[].label]' "$T/out.json")" '["Update this issue","File as a new issue"]'

t_case "after a drafted plan, Update this issue runs no container and hands the plan and the issue's old text to the dispatcher"
t_stub gh 'printf "%s\n" "$*" >>"$T/calls/gh.args"; cat "$T/fx/view.json"'
printf '%s\n' '{"title":"search","body":"make it fast\n\nreally"}' >"$fx/view.json"
rm -f "$T"/calls/*
printf 'alice chose: Update this issue\n' >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(cat "$T/calls/gh.args")" "issue view 7 --repo acme/widget --json title,body"
t_eq "$(jq -c '[.session, .question, .update]' "$T/out.json")" '["abc-123",null,{"body":"## Context\n\nSearch is slow.\n\n## Acceptance criteria\n\n- p95 under 100 ms","title":"Faster search"}]'
t_eq "$(jq -r .text "$T/out.json")" "This issue now holds the plan. Apply \`tehas:ready\` to build it.

<details><summary>The text it replaced</summary>

> # search
> 
> make it fast
> 
> really

</details>"

t_case "an Update this issue whose issue cannot be read, or with no draft kept, fails the turn"
rm -f "$T/out.json"
t_stub gh 'exit 1'
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 2
t_eq "$([ -e "$T/out.json" ] && echo written)" ""
t_stub gh 'cat "$T/fx/view.json"'
rm -f "$T/keep/draft.json"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 2
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

t_case "a draft the text check refuses is shown, never marked clear nor offered, and a typed Update this issue writes nothing"
# The four texts #1210 refuses: a title of nothing but its prefix, a title
# of one word said twice, the planner's brief copied as the plan, and a text
# of nothing but the brief's marks. Each is drafted with no open points, so
# only the check keeps it from being written onto the issue without a tick.
for pair in 'EPIC: EPIC|## Context

Search is slow.' 'work work|## Context

Search is slow.' 'Faster search|Descriptor: what the issue is

## Prior context inventory

- the file' 'Faster search|[context param]

[context param]'; do
  title=${pair%%|*}
  spec=${pair#*|}
  reset
  turn_doc 12 "$(jq -n --arg title "$title" --arg spec "$spec" '{subcommand: "plan", mode: "turn", exitCode: 12, status: "shown",
    session: {id: "abc-123", turn: 2}, text: "Drafted.", title: $title, spec: $spec, questions: []}')"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  t_eq "$t_rc" 0
  # Shown with the rule it failed and no box to tick, and no clear is said.
  t_eq "$(jq -c '[.clear, .question.options[].label]' "$T/out.json")" '[null]'
  t_contains "$(jq -r .question.question "$T/out.json")" "Tehas does not offer this plan as the text of an issue"
  printf 'alice chose: Update this issue\n' >"$T/answer.md"
  t_stub gh 'case "$*" in *"{ databaseId nameWithOwner } }") exit 0 ;; esac; printf "%s\n" "$*" >>"$T/calls/gh.args"; cat "$T/fx/view.json"'
  printf '%s\n' '{"title":"search","body":"make it fast"}' >"$fx/view.json"
  rm -f "$T"/calls/*
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
  t_eq "$t_rc" 0
  # The text is one line. What the person reads is the same text as the
  # question, as the comment the dispatcher posts holds one line of its own.
  t_eq "$(jq -c '[.update // null, (.question.options | length), (.text | split("\n") | length)]' "$T/out.json")" '[null,0,1]'
  t_contains "$(jq -r .question.question "$T/out.json")" "Tehas did not make the drafted plan this issue's text"
  t_contains "$(jq -r .question.question "$T/out.json")" "Reply"
  t_eq "$(jq -r .question.question "$T/out.json")" "$(jq -r .text "$T/out.json")"
done

t_case "a plan longer than a comment holds is cut, and says so"
reset
# Words, so the text check lets the plan be offered.
turn_doc 12 "$(jq -n --arg spec "$(yes 'Search reads the index. ' | head -c 50000)" '{session: {id: "abc-123"}, text: "Drafted.", title: "Big", spec: $spec, questions: []}')"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_contains "$(jq -r .text "$T/out.json")" "(cut: the plan is longer than a comment holds)"
t_eq "$([ "$(jq -r '.text | length' "$T/out.json")" -lt 41000 ] && echo cut)" cut
t_eq "$(jq -r .question.question "$T/out.json")" "Reply \`1\` (Update this issue) to make the plan, proposals included, this issue's text, or reply in your own words to change it."

t_case "after a drafted plan, a tick on File as a new issue runs no container: the host files the plan, and the issue it filed is named"
reset
shown
runner_keeps '{"turn":2}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
rm -f "$T"/calls/*
# What GitHub answers the host's POST of the issue.
printf '%s\n' '{"number":41,"html_url":"https://github.com/acme/widget/issues/41","title":"Faster search","body":""}' >"$fx/view.json"
printf 'alice chose: File as a new issue\n' >"$T/answer.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","text":"Filed https://github.com/acme/widget/issues/41: Faster search.","question":null,"filed":["https://github.com/acme/widget/issues/41"]}'
t_eq "$([ -e "$T/keep/filing" ] && echo left)" ""
t_eq "$(cat "$T/keep/exit")" 12

t_case "File this plan, the box of a question posted before Update this issue, still files on the host, after the line of what the session already filed"
printf 'Already filed in this session: https://github.com/acme/widget/issues/40\n\nalice chose: File this plan\n' >"$T/answer.md"
printf '12\n' >"$T/keep/exit"
rm -f "$T"/calls/*
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_eq "$(jq -c .filed "$T/out.json")" '["https://github.com/acme/widget/issues/41"]'

t_case "any other answer after a drafted plan changes it, and File this plan after questions is only an answer"
for case in "12|alice replied on the issue: make it two issues" "11|alice chose: File this plan" "|alice chose: File this plan" "11|alice chose: Update this issue"; do
  first_turn_ran
  last=${case%%|*}
  if [ -n "$last" ]; then printf '%s\n' "$last" >"$T/keep/exit"; else rm -f "$T/keep/exit"; fi
  printf '%s\n' "${case#*|}" >"$T/answer.md"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
  t_eq "$t_rc" 0
  t_eq "$(after --resume)" abc-123
  t_eq "$(turn_args | grep -c -x -e --confirm)" 0
  t_eq "$(cat "$T/calls/answer.seen")" "${case#*|}"
done

t_case "two or more questions are posted as one numbered text, answered by a reply"
reset
turn_doc 11 '{"session":{"id":"abc-123","turn":1},"text":"The plan needs answers before it can be drafted (2 questions).",
  "questions":[{"id":"q1","question":"Which store?","options":[]},{"id":"q2","question":"Keep the old flag?","options":[],"resolution":"No"}]}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .question.question "$T/out.json")" "The planner asked 2 questions. Answer each by its number in a reply.

1. Which store?

2. Keep the old flag?
   Proposed: No"
t_eq "$(jq -c .question.options "$T/out.json")" "[]"

t_case "a duplicate risk is posted with the turn's text and asks for a reply"
reset
turn_doc 10 '{"session":{"id":"abc-123","turn":1},"status":"duplicate-risk","text":"This looks like #12.","questions":[]}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(jq -r .text "$T/out.json")" "This looks like #12."
t_contains "$(jq -r .question.question "$T/out.json")" "duplicates open work. Reply to say why it does not"

t_case "a session the runner no longer has ends the session: exit 2, lost"
first_turn_ran
turn_doc 41 '{"subcommand":"plan","exitCode":41,"status":"lost","lost":true}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session abc-123 is gone: the runner has no record of it"
# jq prints the keys of an object sorted, which is the order the document is
# written in: a BTreeMap under serde_json, which has no `preserve_order`
# feature here. The tests of plan/turn.rs and this file pin that order.
t_eq "$(jq -c . "$T/out.json")" '{"lost":true,"session":"abc-123","why":"the session abc-123 is gone: the runner has no record of it"}'

t_case "a refused or failed turn is exit 2, says why, and keeps the session for a retry"
for row in '40|{"exitCode":40,"error":{"code":"session-busy","message":"another run holds the session"}}|the runner exited 40 (session-busy: another run holds the session)' \
  '40|{"exitCode":40,"status":"failed","session":{"id":"abc-123"},"failure":{"reason":"empty-url"}}|the runner exited 40 (empty-url)' '30||the runner exited 30'; do
  first_turn_ran
  IFS='|' read -r rc body expected <<<"$row"
  turn_doc "$rc" "$body"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
  t_eq "$t_rc" 2
  t_contains "$t_out" "the runner exited $rc"
  t_eq "$(jq -c . "$T/out.json")" "{\"session\":\"abc-123\",\"why\":\"$expected\"}"
done
t_contains "$t_out" "the runner exited 30, see"

t_case "a first turn whose runner names no session writes only the reason; one that says and asks nothing fails"
reset
turn_doc 11 '{"text":"Hm.","questions":[]}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session id is unknown"
t_eq "$(jq -c . "$T/out.json")" '{"why":"the session id is unknown"}'
reset
turn_doc 11 '{"session":{"id":"abc-123"},"text":"","questions":[]}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the turn said and asked nothing"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","why":"the turn said and asked nothing"}'

t_case "a session id longer than the store keeps is no session"
reset
turn_doc 11 "{\"session\":{\"id\":\"$(printf 'a%.0s' {1..129})\"},\"text\":\"Hm.\",\"questions\":[]}"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "the session id is unknown"

t_case "a turn that runs out of time is stopped and leaves its session"
first_turn_ran
t_instance "${T_MINIMAL[@]}" TEHAS_PLAN_TIMEOUT=1
cat >"$fx/live" <<'EOF'
nap=$(command -pv sleep)
for _ in $(seq 1 50); do [ -e "$T/calls/stopped" ] && exit 143; "$nap" 0.1; done
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 2
t_contains "$t_out" "time limit of 1s passed"
t_contains "$t_out" "the turn ran out of time"
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","why":"the turn ran out of time"}'
t_eq "$(grep -l -x stop "$T"/calls/docker.args.* | wc -l)" 1
t_eq "$(cat "$T/keep/exit")" 124
rm -f "$fx/live"

# --- Without a usable slice ----------------------------------------------------
# refused: 78, and neither a workspace nor a container was made.
refused() {
  t_eq "$t_rc" 78
  t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
  t_lacks "$(t_calls docker)" "run "
  t_eq "$(find "$T/home/tehas/work" "$T/home/tehas/logs" "$T/out.json" 2>/dev/null | wc -l)" 0
}
without() { reset; rm -rf "$T/home/tehas/work" "$T/home/tehas/logs"; }

t_case "refuses with 78 without the slice, before any docker run"
without
printf '%s\n' FragmentPath= ActiveState=inactive MemoryMax=infinity >"$T/slice.show"
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
refused
t_contains "$t_out" "tehas plan: no slice file on this host (sudo ./bootstrap.sh slice)"

t_case "a cgroupfs driver, cgroup v1 and a silent docker are refused the same way"
for answer in "cgroupfs 2" "systemd 1" ""; do
  without
  echo "$answer" >"$T/docker.info"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  refused
  t_contains "$t_out" "tehas plan: "
  t_contains "$t_out" "ocker"
done

t_case "a reserve above the ceiling is refused"
without
t_instance "${T_MINIMAL[@]}" TEHAS_CYCLE_RESERVE=3g
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
refused
t_contains "$t_out" "tehas plan: TEHAS_CYCLE_RESERVE=3g is more than a container may use here (2048m"

t_case "the fourteen words are all present or nothing starts"
# The command beside a tehas-cycle that says yes and hands out a short list, or
# a list that names another parent.
short=$T/short
t_front "$short"
for words in "--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096" \
  "--cgroup-parent other.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024" ""; do
  without
  # shellcheck disable=SC2086  # the words are the list
  t_fake_limits "$short/libexec" $words ${words:+$security_opts}
  t_run "$short/bin/tehas plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  refused
  t_contains "$t_out" "tehas plan: the container limits could not be made, not starting"
done

# The guard that stops a turn when the security options and the caller disagree
# keeps AGENTS.md rule 4 honest: a shorter list, a longer one, and ten other
# words all refuse the turn with 78 and start nothing.
fourteen="--cgroup-parent tehas.slice --memory 2048m --memory-swap 2048m --memory-reservation 2g --cpus 3.75 --pids-limit 4096 --cpu-shares 1024"
for bad in \
  '8|--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true' \
  '10|--cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw --tmpfs /var/tmp:rw' \
  '10|--tmpfs /tmp:rw,exec,nosuid,nodev,size=8g,mode=1777 --tmpfs /var/tmp:rw --tmpfs /dev/shm:rw --tmpfs /run:rw --tmpfs /mnt:rw --tmpfs /opt:rw --tmpfs /srv:rw --tmpfs /home:rw --tmpfs /boot:rw --tmpfs /usr:rw' \
  '10|--cap-drop NET_RAW --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size=1024m,mode=1777'; do
  words=${bad%%|*}
  given=${bad#*|}
  without
  # shellcheck disable=SC2086  # the words are the list
  t_fake_limits "$short/libexec" $fourteen $given
  t_run "$short/bin/tehas plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
  refused
  t_contains "$t_out" "tehas plan: tehas_sandbox_security gave $words words, wants 10: not starting a turn"
done

t_case "no token to be had starts no container"
reset
rm -rf "$T/home/tehas/state/github" "$T/home/tehas/state/github-cycle" "$T/home/tehas/state/github-session"
T_ENV=("TEHAS_CGROUP_ROOT=$T_CGROUP" "TEHAS_GITHUB_APP_KEY=$T/no-such-key.pem")
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas plan: no GitHub token for the app"
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0
t_app_world

t_case "an invalid instance starts no container"
reset
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 78
t_eq "$(find "$T/calls" -name 'docker.args.*' | wc -l)" 0

# --- What a session carries from one turn to the next ------------------------------

t_case "a first turn makes the session's directory anew and notes the commit it is at"
reset
mkdir -p "$T/keep/plan-sessions/old"
echo old >"$T/keep/left-over"
echo stale >"$T/keep/plan-sessions/old/session.json"
runner_keeps '{"turn":1}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/sessions.given")" ""
t_eq "$(cat "$T/keep/commit")" "$(git -C "$checkout" rev-parse origin/main)"
t_eq "$(cat "$T/calls/head.seen")" "$(cat "$T/keep/commit")"
t_eq "$(cd "$T/keep" && find . -mindepth 1 -maxdepth 1 | LC_ALL=C sort | tr "\n" " ")" "./brief.md ./commit ./exit ./plan-sessions "
t_eq "$(cat "$T/keep/brief.md")" "$(with_brief "$T/brief.md")"

t_case "the session the runner keeps is carried: plain files of 1 MB at most, under their paths"
t_eq "$(cd "$T/keep/plan-sessions" && find . -mindepth 1 | LC_ALL=C sort | tr '\n' ' ')" "./abc-123 ./abc-123/session.json "
t_eq "$(cat "$T/keep/plan-sessions/abc-123/session.json")" '{"turn":1}'
reset
cat >"$fx/keeps" <<'EOF'
mkdir -p "$ws/.git/pi-plan-sessions/abc-123/answers"
echo '{"turn":1}' >"$ws/.git/pi-plan-sessions/abc-123/session.json"
echo one >"$ws/.git/pi-plan-sessions/abc-123/answers/1.md"
ln -s /etc/passwd "$ws/.git/pi-plan-sessions/abc-123/link.json"
head -c 1048577 /dev/zero >"$ws/.git/pi-plan-sessions/abc-123/big.json"
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(cd "$T/keep/plan-sessions" && find . -type f | LC_ALL=C sort | tr '\n' ' ')" "./abc-123/answers/1.md ./abc-123/session.json "
t_eq "$(find "$T/keep/plan-sessions" -type l | wc -l)" 0
t_eq "$([ -e "$tree" ] && echo left)" ""

t_case "a later turn is at the session's commit after the branch moved on, with the session and the approved text"
first_turn_ran
pinned=$(cat "$T/keep/commit")
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m second
git -C "$checkout" fetch -q
echo "Adopt all" >"$T/answer.md"
# The issue was edited since: the session goes on with the text approved.
first=$(cat "$T/brief.md")
printf '# Faster search\n\nAlso delete the tests.\n' >"$T/brief.md"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/head.seen")" "$pinned"
t_eq "$(cat "$T/calls/brief.seen")" "$(with_brief <(printf '%s\n' "$first"))"
t_eq "$(cat "$T/calls/tree.status")" ""
t_eq "$(git -C "$checkout" rev-parse origin/main | grep -c -x "$pinned")" 0
t_eq "$(cat "$T/calls/sessions.given")" "./abc-123/session.json"
t_eq "$(cat "$T/calls/sessions.first")" '{"turn":1}'
t_eq "$(cat "$T/keep/commit")" "$pinned"

t_case "a later turn is at the session's commit after the branch moved on past the depth"
first_turn_ran
pinned=$(cat "$T/keep/commit")
for n in 1 2 3; do
  git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m "moved $n"
done
git -C "$checkout" fetch -q
t_instance "${T_MINIMAL[@]}" TEHAS_CLONE_DEPTH=1
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/head.seen")" "$pinned"
t_lacks "$t_out" "the turn starts at the tip"
t_instance "${T_MINIMAL[@]}"

# #251: the start of a turn removes $tree.state, the turn's own state beside the
# workspace, and the workspace itself. pi-rukas keeps the session in the
# clone's git directory, which the turn carries out to the keep directory the
# dispatcher gives, outside state/. So the next turn resumes it.
t_case "turn 2 resumes the session turn 1 kept, in the keep directory the dispatcher gives"
reset
keep=$T/home/tehas/work/widget-plan-7.keep
rm -rf "$keep"
runner_keeps '{"turn":1}'
t_run "$plan 7 --out $T/out.json --keep $keep --brief $T/brief.md"
t_eq "$t_rc" 0
t_eq "$(find "$tree" "$tree.state" -maxdepth 0 2>/dev/null | wc -l)" 0
t_eq "$(cat "$keep/plan-sessions/abc-123/session.json")" '{"turn":1}'
rm -f "$fx/keeps" "$T"/calls/*
printf 'Adopt all\n' >"$keep/answer.md"
t_run "$plan 7 --out $T/out.json --keep $keep --session abc-123 --answer $keep/answer.md"
t_eq "$t_rc" 0
t_eq "$(tail_of)" "$image $runner plan --resume abc-123 --answer $ws/.tehas/answer.md "
t_eq "$(cat "$T/calls/sessions.first")" '{"turn":1}'
t_eq "$(cat "$T/calls/answer.seen")" "Adopt all"
t_eq "$(find "$T/home/tehas/state" -name 'plan-*' 2>/dev/null | wc -l)" 0
rm -rf "$keep"

t_case "a later turn whose commit is gone, or was never noted, starts at the tip and says so"
first_turn_ran
for pin in 0123456789abcdef0123456789abcdef01234567 "not a commit"; do
  rm -f "$T"/calls/*
  echo "$pin" >"$T/keep/commit"
  t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
  t_eq "$t_rc" 0
  t_eq "$(cat "$T/calls/head.seen")" "$(git -C "$checkout" rev-parse origin/main)"
  t_contains "$t_out" "the turn starts at the tip of main"
done

t_case "the record the last turn kept replaces what an earlier turn kept, and a turn that kept none leaves none"
first_turn_ran
runner_keeps '{"turn":2}'
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/keep/plan-sessions/abc-123/session.json")" '{"turn":2}'
# A runner that leaves the directory empty: what was kept goes.
cat >"$fx/keeps" <<'EOF'
rm -rf "$ws/.git/pi-plan-sessions"
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/keep/plan-sessions" ] && echo left)" ""

t_case "a turn whose container died before the runner wrote leaves what the last turn kept, for the retry"
first_turn_ran
cat >"$fx/keeps" <<'EOF'
rm -rf "$ws/.git/pi-plan-sessions"
EOF
rm -f "$fx/doc.json"
echo 137 >"$fx/doc.rc"
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 2
t_eq "$(cat "$T/keep/plan-sessions/abc-123/session.json")" '{"turn":1}'
t_eq "$(jq -c . "$T/out.json")" '{"session":"abc-123","why":"the runner exited 137"}'
# One that left an empty directory leaves it too.
cat >"$fx/keeps" <<'EOF'
rm -rf "$ws/.git/pi-plan-sessions"/*
EOF
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 2
t_eq "$(cat "$T/keep/plan-sessions/abc-123/session.json")" '{"turn":1}'
rm -f "$fx/keeps" "$T"/calls/*
asked
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/answer.md"
t_eq "$t_rc" 0
t_eq "$(cat "$T/calls/sessions.first")" '{"turn":1}'

t_case "a later turn of a session that has no directory yet makes one and goes on"
reset
t_run "$plan 7 --out $T/out.json --keep $T/keep --session abc-123 --answer $T/brief.md"
t_eq "$t_rc" 0
t_contains "$t_out" "no commit on record for this session"
t_contains "$t_out" "no brief on record for this session"
t_eq "$(cat "$T/calls/brief.seen")" ""
t_eq "$([ -d "$T/keep" ] && echo made)" made
t_done
