#!/usr/bin/env bash
# Tests for libexec/dispatch: running without the test seams, filling the
# slots, stopping finished cycles, and the retry after a provider outage.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh"

mesh_curl

# Outside the tests nothing names the scripts: the dispatcher runs the ones in
# its own directory, whatever PATH holds, and starts a cycle through systemd.
bare_tree() {
  # The cgroup root is the one seam left: the slice's use cannot be read from
  # the machine the tests run on.
  T_ENV=("$slice_env")
  tree=$T/tree
  rm -rf "$tree"
  mkdir -p "$tree/libexec" "$tree/lib"
  cp "$dispatch" "$tree/libexec/dispatch"
  cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/cycles.sh" "$T_ROOT/lib/labels.sh" "$T_ROOT/lib/questions.sh" "$T_ROOT/lib/planning.sh" "$T_ROOT/lib/slice.sh" "$T_ROOT/lib/attachments.sh" "$tree/lib/"
  for name in status index plan; do cp "$T/bin/fake-$name" "$tree/libexec/$name"; done
  for name in tehas tehasbot tehas-work tehas-status tehas-index tehas-plan; do t_stub "$name"; done
  # Runs the unit's command at once, after its own options.
  t_stub systemd-run 'while [ "$#" -gt 0 ]; do case "$1" in --unit | -p) shift 2 ;; -*) shift ;; *) break ;; esac; done
"$@"'
}

t_case "without the test seams it runs the scripts beside it and starts the unit"
reset
bare_tree
t_stub systemctl 'case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") echo loaded ;;
  "--user start") n=${3#tehas-cycle@}
    : >"$TEHAS_HOME/state/cycles/${n%.service}.taken" ;;
esac'
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl)" "show tehas.slice -p FragmentPath -p ActiveState -p MemoryMax
--user show -p LoadState --value tehas-cycle@7.service
--user show -p ActiveState --value tehas-index.service
--user start tehas-cycle@7.service"
t_eq "$(t_calls docker)" "info --format {{.CgroupDriver}} {{.CgroupVersion}}"
t_contains "$(t_calls fake-index)" "--before-cycle"
t_contains "$(t_calls systemd-run)" "--user --quiet --collect --unit tehas-index -p RuntimeMaxSec=2400 /bin/bash -c"
t_eq "$(find "$T/home/tehas/state/dispatch.again" 2>/dev/null | wc -l)" 0
t_eq "$(t_calls fake-status | wc -l)" 2
for name in tehas tehasbot tehas-work tehas-status tehas-index tehas-plan; do t_eq "$(t_calls "$name")" ""; done

t_case "without the test seams a planning turn runs the script beside it too"
to_plan
bare_tree
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 0
t_contains "$(plan_calls)" "7 --out $record.out --keep $record.keep --brief $record.brief"
t_contains "$(t_calls systemd-run)" "--user --quiet --collect --unit tehas-plan-7 -p RuntimeMaxSec=4500 /bin/bash -c"
t_contains "$(t_calls gh)" "- [ ] Adopt all"
for name in tehas tehasbot tehas-plan; do t_eq "$(t_calls "$name")" ""; done

t_case "a unit systemd cannot load changes no label and starts nothing, exit 1"
reset
bare_tree
t_stub systemctl 'case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") echo not-found ;;
esac'
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: systemd cannot load tehas-cycle@.service (LoadState: not-found): run install.sh and its units step, and run tehas dispatch as the tehas user's own session"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls systemctl)" "start"
t_eq "$(t_calls fake-index)" ""

t_case "a systemd that gives no answer changes no label and starts nothing, exit 1"
reset
bare_tree
t_stub systemctl 'case "$1 $2" in
  "show tehas.slice") cat "$T/fx/slice.show" ;;
  "--user show") exit 1 ;;
esac'
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: systemd cannot load tehas-cycle@.service (LoadState: no answer): run install.sh"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls systemctl)" "start"

# --- The slots are filled ---------------------------------------------------
new_rukas=$(printf 'a%.0s' {1..40})

t_case "a pi-rukas no cycle has run before starts as many cycles as the slots allow"
reset
slots 4
echo "$new_rukas|1" >"$fx/image.label"
ready 7 8 9 10 11 12
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 9 10 "
t_lacks "$t_out" "is new on this host"
t_eq "$(find "$T/home/tehas/state/rukas-proven" 2>/dev/null | wc -l)" 0
rm -f "$fx/image.label"

t_case "with one cycle running and five waiting, one poll starts three"
reset
slots 4
ready 7 8 9 10 11
hold 12
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 9 "
t_lacks "$t_out" "holding"
release

# A running cycle whose lock a process holds, which fake-stop ends as
# systemctl stop ends the unit. It took its lock now.
hold_proc() { # <issue>
  mkdir -p "$T/home/tehas/state/cycles"
  : >"$T/home/tehas/state/cycles/$1.taken"
  flock -o "$T/home/tehas/state/cycles/$1.lock" sleep 30 >/dev/null 2>&1 &
  echo "$!" >"$fx/holder-$1"
  for _ in $(seq 50); do
    flock -n "$T/home/tehas/state/cycles/$1.lock" true || break
    sleep 0.1
  done
}
stop_by_kill() {
  t_stub fake-stop 'kill "$(cat "$T/fx/holder-$1")"'
  T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
}
# The bot's own pull request unless an author is named; - for one GitHub
# reports no author for.
bot_pr() { # <issue> <state> <created at> [<author login>]
  jq -n --arg n "$1" --arg st "$2" --arg at "$3" --arg au "${4:-}" '[{url: "https://github.com/acme/widget/pull/90", state: $st,
    body: "Fixes #\($n)", headRefName: "feature/issue-\($n)-x", createdAt: $at}
    + (if $au == "" then {} elif $au == "-" then {author: null}
       else {author: {login: $au}} end)]' >"$fx/prlist.json"
}
# The cycle's lock is still held: nothing stopped it, and its slot is still taken.
still_holding() { # <issue>
  if flock -n "$T/home/tehas/state/cycles/$1.lock" true; then
    t_bad "the cycle for #$1 no longer holds its lock"
  fi
}

t_case "a running cycle whose issue is closed is stopped, and its slot is filled on the same poll"
reset
stop_by_kill
ready 7
hold_proc 9
echo '{"number":9,"state":"closed"}' >"$fx/work-9.json"
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
t_contains "$t_out" "stop #9: its issue is closed"
t_eq "$(cat "$T/home/tehas/state/cycles/9.done")" closed
t_eq "$(t_calls fake-start)" "7"

t_case "a running cycle whose pull request is merged is stopped, and its slot is filled on the same poll"
reset
stop_by_kill
ready 7
hold_proc 9
bot_pr 9 MERGED 2099-01-01T00:00:00Z
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
t_contains "$t_out" "stop #9: its pull request https://github.com/acme/widget/pull/90 is merged"
t_eq "$(cat "$T/home/tehas/state/cycles/9.done")" merged
t_eq "$(t_calls fake-start)" "7"

t_case "a pull request closed without a merge stops the cycle too"
reset
stop_by_kill
echo '[]' >"$fx/ready.json"
hold_proc 9
bot_pr 9 CLOSED 2099-01-01T00:00:00Z
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
t_eq "$(cat "$T/home/tehas/state/cycles/9.done")" pr-closed

t_case "an open pull request, one of an earlier cycle, or one of another issue stops nothing"
reset
stop_by_kill
ready 7
hold_proc 9
for pr in "9 OPEN 2099-01-01T00:00:00Z" "9 MERGED 2000-01-01T00:00:00Z" "8 MERGED 2099-01-01T00:00:00Z"; do
  # shellcheck disable=SC2086  # three words
  bot_pr $pr
  t_run "$dispatch --dry-run"
  t_lacks "$t_out" "would stop"
done
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(t_calls fake-start)" ""
t_contains "$(t_calls gh)" "pr list --repo acme/widget --author tehas-bot --state all"
kill "$(cat "$fx/holder-9")"

# A poll that cannot ask stops nothing: a cycle is never given up on a guess.
t_case "a poll that cannot read the bot's pull requests stops nothing and leaves the cycle holding its lock"
reset
stop_by_kill
ready 7
hold_proc 9
bot_pr 9 MERGED 2099-01-01T00:00:00Z
: >"$fx/pr-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh pr list failed"
t_lacks "$t_out" "stop #9"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(find "$T/home/tehas/state/cycles/9.done" 2>/dev/null | wc -l)" 0
still_holding 9
kill "$(cat "$fx/holder-9")"

t_case "a poll that cannot read the issue stops nothing and leaves the cycle holding its lock"
reset
stop_by_kill
ready 7
hold_proc 9
echo '{"number":9,"state":"closed"}' >"$fx/work-9.json"
: >"$fx/work-9-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh api repos/acme/widget/issues/9 failed"
t_lacks "$t_out" "stop #9"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(find "$T/home/tehas/state/cycles/9.done" 2>/dev/null | wc -l)" 0
still_holding 9
kill "$(cat "$fx/holder-9")"

t_case "a merged pull request of another account, or one with no author, stops nothing; the bot's own stops it"
reset
stop_by_kill
ready 7
hold_proc 9
for au in someone-else -; do
  bot_pr 9 MERGED 2099-01-01T00:00:00Z "$au"
  t_run "$dispatch --dry-run"
  t_lacks "$t_out" "would stop"
  t_eq "$(t_calls fake-stop)" ""
done
bot_pr 9 MERGED 2099-01-01T00:00:00Z tehas-bot
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service: its pull request https://github.com/acme/widget/pull/90 is merged"
t_eq "$(t_calls fake-stop)" ""
kill "$(cat "$fx/holder-9")"

t_case "a dry run says it would stop a finished cycle, and stops nothing"
reset
stop_by_kill
hold_proc 9
echo '{"number":9,"state":"closed"}' >"$fx/work-9.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service: its issue is closed"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(find "$T/home/tehas/state/cycles/9.done" 2>/dev/null | wc -l)" 0
kill "$(cat "$fx/holder-9")"

# The pause switches say what a poll may start, not what it may clean up: a
# paused factory still gives up the work it has finished and still clears a mark
# with no cycle behind it.
t_case "a paused poll stops a cycle whose issue is closed and starts nothing"
reset
stop_by_kill
pause_flag '{"by":"boss","at":"2026-10-15T10:00:00Z","reason":"waiting for the gate"}'
hold_proc 9
echo '{"number":9,"state":"closed"}' >"$fx/work-9.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-stop)" "9"
t_contains "$t_out" "stop #9: its issue is closed"
t_eq "$(cat "$T/home/tehas/state/cycles/9.done")" closed
t_eq "$(t_calls fake-start)" ""

t_case "a paused dry run says it would stop a finished cycle and stops nothing"
reset
stop_by_kill
pause_flag '{"by":"boss","at":"2026-10-15T10:00:00Z","reason":""}'
hold_proc 9
echo '{"number":9,"state":"closed"}' >"$fx/work-9.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service: its issue is closed"
t_contains "$t_out" "paused: the flag tehas pause wrote"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(t_calls fake-start)" ""
kill "$(cat "$fx/holder-9")"

t_case "a paused poll still parks an issue marked tehas:running with no cycle behind it"
reset
pause_flag '{"by":"boss","at":"2026-10-15T10:00:00Z","reason":""}'
echo '[{"number":9}]' >"$fx/marked.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "park #9: it is marked tehas:running and no cycle holds its lock"
t_eq "$(t_calls fake-start)" ""

t_case "with no cycle running the bot's pull requests are not asked for"
reset
t_run "$dispatch"
t_lacks "$(t_calls gh)" "pr list"

t_case "a poll that starts fewer than it could logs exactly one reason, on every such poll"
reset
slots 4 TEHAS_MAX_AWAITING=3
echo '[{"number":1}]' >"$fx/awaiting.json"
ready 7 8 9 10 11 12
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_eq "$(grep -c 'holding' <<<"$t_out")" 1
t_contains "$t_out" "holding: 2 of 4 free slots start: 1 issues await merge and 0 cycles are running (limit 3)"
t_eq "$(jq -r .why "$dispatch_json")" awaiting
rm -f "$T"/calls/fake-start
rm -rf "$T/home/tehas/state/cycles"
t_run "$dispatch"
t_eq "$(grep -c 'holding' <<<"$t_out")" 1

t_case "the slice's room, when it is the limit, is the one reason"
reset
slots 4
other $((15 * gib))
ready 7 8 9 10
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_eq "$(grep -c 'holding' <<<"$t_out")" 1
t_contains "$t_out" "holding: 2 of 4 free slots start: tehas.slice has room for 2 more"
t_eq "$(jq -r .why "$dispatch_json")" room

t_case "a poll that starts as many as wait says no reason"
reset
slots 4 TEHAS_MAX_AWAITING=3
echo '[{"number":1}]' >"$fx/awaiting.json"
ready 7 8
t_run "$dispatch"
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_lacks "$t_out" "holding"
t_eq "$(jq -r .why "$dispatch_json")" ""

# --- A cycle a provider outage parked -------------------------------------------
# Issue 7 was parked by an outage after a cycle alice started at 02:00, its
# question unticked, and the endpoint answers again.
outage_parked() { # [<retry>]
  parked
  comments 7 "$(park_body ' ' ' ')" ""
  mesh '{"data":[{"id":"test-model"}]}'
  mkdir -p "$cycles_dir"
  jq -n --argjson s "$(date -d 2026-01-01T02:00:00Z +%s)" --argjson r "${1:-true}" \
    '{cap: "step-failed:adversarial", reason: "Connection error.", provider: true, retry: $r, started: $s}' >"$cycles_dir/7.park"
  echo alice >"$cycles_dir/7.by"
  echo '{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:question"}]}' >"$fx/issue-7.json"
}

t_case "a cycle a provider outage parked starts again once, without a tick, and the issue is told"
outage_parked
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:question"
t_lacks "$(t_calls gh)" "--remove-label needs-human-attention"
t_contains "$(t_calls gh)" "Tehas starts this issue again on its own: its last cycle was parked by a provider outage, and the model endpoint answers again. It does this once."
t_contains "$(t_calls gh)" "authorized by @alice's start of the cycle a provider outage parked"
t_contains "$t_out" "start #7 (requeued by alice)"
t_lacks "$(t_calls gh)" "api -X PATCH"

t_case "the park is used up, and the start is marked as the retry for its cycle"
t_eq "$(find "$cycles_dir/7.park" 2>/dev/null | wc -l)" 0
t_eq "$(find "$cycles_dir/7.requeued" 2>/dev/null | wc -l)" 1
t_eq "$(cat "$cycles_dir/7.by")" alice
t_eq "$([ -e "$park_id" ] && echo kept)" ""
t_eq "$(jq -r '"\(.how) \(.by) \(.at)"' "$cycles_dir/7.approved.json")" "outage-retry alice 2026-01-01T02:00:00Z"

t_case "a start a person asked for is not the retry, and says who asked"
reset
mkdir -p "$cycles_dir"
: >"$cycles_dir/7.requeued"
echo '{"retry":true}' >"$cycles_dir/7.park"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_eq "$(find "$cycles_dir/7.requeued" "$cycles_dir/7.park" 2>/dev/null | wc -l)" 0
t_eq "$(cat "$cycles_dir/7.by")" alice

t_case "a park that asks for no retry starts nothing and stays"
outage_parked false
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(find "$cycles_dir/7.park" | wc -l)" 1

t_case "the retry waits while the endpoint does not answer, or the slots are full, and its park stays"
outage_parked
mesh
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(find "$cycles_dir/7.park" | wc -l)" 1
outage_parked
hold 9
t_run "$dispatch"
t_eq "$(t_calls fake-start)" ""
t_eq "$(find "$cycles_dir/7.park" | wc -l)" 1
release

t_case "a dry run says what it would do and changes nothing"
outage_parked
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7 (retry after a provider outage, started by alice)"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_eq "$(find "$cycles_dir/7.park" | wc -l)" 1

t_case "no retry when its starter is no longer allowed, the text was changed by another, or the issue is not parked"
for why in stranger edited closed unparked nobody; do
  outage_parked
  case "$why" in
    stranger) echo mallory >"$cycles_dir/7.by" ;;
    nobody) rm -f "$cycles_dir/7.by" ;;
    edited) echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-01T03:00:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json" ;;
    closed) echo '{"state":"CLOSED","labels":[{"name":"needs-human-attention"}]}' >"$fx/issue-7.json" ;;
    unparked) echo '{"state":"OPEN","labels":[]}' >"$fx/issue-7.json" ;;
  esac
  t_run "$dispatch"
  t_eq "$(t_calls fake-start)" ""
  t_lacks "$(t_calls gh)" "issue edit"
  t_contains "$t_out" "skip #7: its retry after a provider outage is not started; it waits for a person"
  t_eq "$(find "$cycles_dir/7.park" 2>/dev/null | wc -l)" 0
done

t_case "an issue a person labelled tehas:ready is left to that label"
outage_parked
echo '{"state":"OPEN","labels":[{"name":"needs-human-attention"},{"name":"tehas:ready"}]}' >"$fx/issue-7.json"
echo '[{"number":7,"labels":[{"name":"tehas:ready"},{"name":"needs-human-attention"}]}]' >"$fx/ready.json"
t_run "$dispatch"
t_eq "$(t_calls fake-start)" "7"
t_contains "$t_out" "start #7 (labelled by alice)"
t_eq "$(find "$cycles_dir/7.requeued" "$cycles_dir/7.park" 2>/dev/null | wc -l)" 0
rm -f "$T/home/.pi/agent/models.json" "$fx/mesh.json"

t_done
