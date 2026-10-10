#!/usr/bin/env bash
# Tests for libexec/dispatch against a stub gh and a scratch checkout whose
# default branch is main.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

dispatch="$T_ROOT/libexec/dispatch"
fx=$T/fx
mkdir -p "$fx"
checkout=$T/home/tehas/repos/widget

# gh answers from fixture files; every call is recorded by t_stub.
t_stub gh '
case "$1 $2" in
  "repo view") cat "$T/fx/repo.json" || exit 1 ;;
  "issue view") cat "$T/fx/control.json" || exit 1 ;;
  "issue list")
    case "$*" in
      *awaiting-merge*) cat "$T/fx/awaiting.json" || exit 1 ;;
      *tehas:ready*) cat "$T/fx/ready.json" || exit 1 ;;
      *) echo "[]" ;;
    esac ;;
  "api repos/acme/widget/issues/"*)
    n=${2#repos/acme/widget/issues/}
    cat "$T/fx/events-${n%/events*}.json" || exit 1 ;;
  "api graphql")
    n=$(printf "%s\n" "$@" | sed -n "s/^number=//p")
    cat "$T/fx/edit-$n.json" 2>/dev/null || cat "$T/fx/edit.json" || exit 1 ;;
  "issue edit") [ ! -e "$T/fx/edit-fails" ] || exit 1 ;;
  "pr list") cat "$T/fx/pr.txt" 2>/dev/null ;;
esac'
# Stands in for systemctl --user start tehas-cycle@<issue>.service. A real
# cycle says it holds its lock by writing the .taken file. This one fails or
# stays silent when a case asks it to.
default_start() {
  t_stub fake-start '[ ! -e "$T/fx/start-fails" ] || exit 1
[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/start.fd8"
[ -e "$T/fx/start-silent" ] || : >"$TEHAS_HOME/state/cycles/$1.taken"'
}
default_start
t_stub fake-status
# Notes what the checkout was at and whether a cycle had already started.
t_stub fake-index 'git -C "$T/home/tehas/repos/widget" rev-parse HEAD >"$T/calls/index.head"
[ ! -e "$T/calls/fake-start" ] || : >"$T/calls/index.late"
[ ! -e "$T/fx/index-fails" ] || exit 1'

reset() {
  rm -f "$T"/calls/* "$fx"/edit-* "$fx"/start-* "$fx"/events-*
  rm -rf "$T/home/tehas/state/cycles"
  echo '{"data":{"repository":{"issue":{"lastEditedAt":null,"editor":null}}}}' >"$fx/edit.json"
  echo '{"nameWithOwner":"acme/widget"}' >"$fx/repo.json"
  echo '{"labels":[]}' >"$fx/control.json"
  echo '[]' >"$fx/awaiting.json"
  echo '[{"number":7,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}}]' >"$fx/events-7.json"
  T_ENV=(TEHAS_CYCLE_START_CMD=fake-start TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index)
  t_instance "${T_MINIMAL[@]}"
}
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

# A product repository whose default branch is main, and the instance's clone.
upstream=$T/git/acme/widget
mkdir -p "$upstream"
git init -q -b main "$upstream"
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
git clone -q "$upstream" "$checkout"

t_case "a dry run picks the issue an allowed account labelled"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7 (labelled by alice)"
t_lacks "$(t_calls gh)" "issue edit"
t_eq "$(t_calls fake-start)" ""

t_case "an issue labelled by another account is skipped"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'mallory', not an allowed account"
t_contains "$t_out" "0 cycles running, room for 1 more"
t_contains "$t_out" "nothing to do"

t_case "the most recent labelling decides, not an earlier one"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"unlabeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"mallory"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"

t_case "a login that only contains an allowed one is not allowed"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice2"}}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"

t_case "a label event with no actor is not an allowed account"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: tehas:ready was applied by 'unknown'"
t_contains "$t_out" "nothing to do"

t_case "an issue with no label event at all is skipped"
reset
echo '[]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7"
t_contains "$t_out" "nothing to do"

for odd in "alice  bob" "alice " " alice" " "; do
  t_case "a labeller list with stray spaces ('$odd') is refused, not guessed at"
  reset
  printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS='$odd'" >"$TEHAS_INSTANCE_DIR/instance.env"
  echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":null}]' >"$fx/events-7.json"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 78
  t_lacks "$t_out" "would run a cycle"
  t_eq "$(t_calls gh)" ""
done

t_case "when the pause check cannot reach GitHub nothing is dispatched"
reset
rm "$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue view failed"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "when the awaiting-merge count cannot be read nothing is dispatched"
reset
rm "$fx/awaiting.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh issue list failed"
t_eq "$(t_calls fake-start)" ""

t_case "when the label events cannot be read the issue is not taken"
reset
rm "$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a checkout of another repository stops everything"
reset
git -C "$checkout" remote set-url origin "$T/git/acme/other"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "is a checkout of"
t_contains "$t_out" "not of acme/widget"
t_eq "$(t_calls gh)" ""
git -C "$checkout" remote set-url origin "$upstream"

for url in "https://github.com/acme/widget.git" "https://github.com/acme/widget" "git@github.com:acme/widget.git" "ssh://git@github.com/acme/widget/"; do
  t_case "origin $url is recognised as acme/widget"
  reset
  git -C "$checkout" remote set-url origin "$url"
  t_run "$dispatch --dry-run"
  t_eq "$t_rc" 0
  t_contains "$t_out" "would run a cycle for #7"
done
t_case "origin ending in a longer name is not acme/widget"
reset
git -C "$checkout" remote set-url origin "https://github.com/notacme/widget.git"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 78
git -C "$checkout" remote set-url origin "$upstream"

t_case "a missing checkout stops before any label changes, even with TEHAS_BRANCH set"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_BRANCH=main TEHAS_WORK_REPO="$T/nowhere"
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "no product checkout at $T/nowhere"
t_eq "$(t_calls gh)" ""
t_eq "$([ -e "$T/nowhere" ] && echo created)" ""

t_case "a renamed repository stops the poll and says its new name"
reset
echo '{"nameWithOwner":"acme/gadget"}' >"$fx/repo.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: acme/widget is now acme/gadget on GitHub: set TEHAS_REPO"
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue list"

t_case "a repository that cannot be looked up stops the poll"
reset
rm "$fx/repo.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: gh repo view failed"
t_lacks "$t_out" "nothing to do"

t_case "the repository's name is compared without regard to case"
reset
echo '{"nameWithOwner":"Acme/Widget"}' >"$fx/repo.json"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"

t_case "the pause label on the control issue stops dispatch"
reset
echo '{"labels":[{"name":"tehas:pause"}]}' >"$fx/control.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "paused: issue #1 carries tehas:pause"

t_case "an invalid instance makes no call at all and exits 78"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an empty labeller list does not widen who may label"
reset
printf '%s\n' "${T_MINIMAL[@]}" "TEHAS_ALLOWED_LABELERS=''" >"$TEHAS_INSTANCE_DIR/instance.env"
t_run "$dispatch --dry-run"
t_eq "$t_rc" 78
t_eq "$(t_calls gh)" ""

t_case "a real poll puts the checkout on main and fast-forwards it"
reset
git -C "$upstream" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m second
git -C "$checkout" checkout -q -b feature/left-over
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(git -C "$checkout" branch --show-current)" "main"
t_eq "$(git -C "$checkout" rev-parse HEAD)" "$(git -C "$upstream" rev-parse main)"
t_lacks "$t_out" "WARN"
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "the code index is refreshed after the fetch and before the cycle"
t_eq "$(t_calls fake-index)" "--before-cycle"
t_eq "$(cat "$T/calls/index.head")" "$(git -C "$upstream" rev-parse main)"
t_eq "$(find "$T/calls/index.late" 2>/dev/null | wc -l)" 0

t_case "an index that cannot be refreshed is a warning, and the cycle still runs"
reset
: >"$fx/index-fails"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the code index was not refreshed"
t_eq "$(t_calls fake-start)" "7"
rm "$fx/index-fails"

t_case "a dry run refreshes no index"
reset
t_run "$dispatch --dry-run"
t_eq "$(t_calls fake-index)" ""

t_case "a real poll does not start an issue the bot labelled"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"}},
       {"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"the-bot"}}]' >"$fx/events-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"

t_case "every page of the label events is read, a hundred at a time"
t_contains "$(t_calls gh)" "api repos/acme/widget/issues/7/events?per_page=100 --paginate"

t_case "a cycle does not start when the issue cannot be marked as running"
reset
: >"$fx/edit-fails"
t_run "$dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "could not mark #7 as running"
t_eq "$(t_calls fake-start)" ""
rm -f "$fx/edit-fails"

t_case "a body edited by its author after the label was applied is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'mallory' after alice applied tehas:ready"

t_case "an edit before the label was applied is what the label approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T09:59:59Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7"

t_case "an edit after the label by an allowed account keeps the approval"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"alice"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #7"

t_case "an edit after the label by nobody known is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":null}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'unknown'"

t_case "a title changed after the label by another account is not approved"
reset
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"},
       {"event":"renamed","actor":{"login":"mallory"},"created_at":"2026-01-02T10:01:00Z"}]' >"$fx/events-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "skip #7: its text was changed by 'mallory'"

t_case "an edited issue does not hold up the one behind it"
reset
echo '[{"number":7,"labels":[{"name":"tehas:ready"}]},{"number":8,"labels":[{"name":"tehas:ready"}]}]' >"$fx/ready.json"
echo '[{"event":"labeled","label":{"name":"tehas:ready"},"actor":{"login":"alice"},"created_at":"2026-01-02T10:00:00Z"}]' >"$fx/events-7.json"
cp "$fx/events-7.json" "$fx/events-8.json"
echo '{"data":{"repository":{"issue":{"lastEditedAt":"2026-01-02T10:05:00Z","editor":{"login":"mallory"}}}}}' >"$fx/edit-7.json"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would run a cycle for #8"
rm -f "$fx/events-8.json"

t_case "when the edit record cannot be read the issue is not taken"
reset
rm -f "$fx/edit.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"

t_case "a pause answer that cannot be understood dispatches nothing"
reset
echo '{"labels":null}' >"$fx/control.json"
t_run "$dispatch"
t_eq "$t_rc" 1
t_eq "$(t_calls fake-start)" ""

t_case "TEHAS_BRANCH overrides the checkout's default branch"
reset
git -C "$checkout" branch -q release main
git -C "$upstream" branch -q release main
git -C "$checkout" fetch -q origin
t_instance "${T_MINIMAL[@]}" TEHAS_BRANCH=release
t_run "$dispatch"
t_eq "$(git -C "$checkout" branch --show-current)" "release"

t_case "a checkout without origin/HEAD stops before any label changes"
reset
git -C "$checkout" remote set-head origin -d
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "default branch"
t_lacks "$(t_calls gh)" "issue edit"
git -C "$checkout" remote set-head origin main

t_case "--init-labels is libexec/labels now: here it is an unknown argument"
reset
t_run "$dispatch --init-labels"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --init-labels"
t_eq "$(t_calls gh)" ""

t_case "the cycle is started for the issue and told the labeller in a comment"
reset
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" "7"
t_contains "$(t_calls gh)" "Tehas cycle started on"
t_contains "$(t_calls gh)" "authorized by @alice"

t_case "the poll's lock is not passed on to the cycle it starts"
t_eq "$(find "$T/calls/start.fd8" 2>/dev/null | wc -l)" 0

t_case "the status is refreshed before the poll and after the start"
t_eq "$(t_calls fake-status | wc -l)" 2

t_case "for one issue: the running label, then the start comment, then the start"
reset
# fake-order notes what the gh calls held when the start was asked for.
t_stub fake-order 'grep -q -e "--add-label tehas:running" "$T/calls/gh" && : >"$T/calls/order.label"
grep -q "^issue comment $1 " "$T/calls/gh" && : >"$T/calls/order.comment"
: >"$TEHAS_HOME/state/cycles/$1.taken"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-order TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index)
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$T/calls/order.label" "$T/calls/order.comment" 2>/dev/null | wc -l)" 2
label_at=$(grep -n -e "--add-label tehas:running" "$T/calls/gh" | head -n 1 | cut -d: -f1)
comment_at=$(grep -n "^issue comment 7 " "$T/calls/gh" | head -n 1 | cut -d: -f1)
t_eq "$([ "$label_at" -lt "$comment_at" ] && echo before)" "before"

t_case "the poll does not wait for the cycle: it returns while the cycle lives, with its own lock free"
reset
# The cycle outlives the poll and holds its own lock. It must not hold the
# poll's, so the next poll can run. Its output is detached from the test's pipe.
t_stub fake-live '( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; exec sleep 8 ) >/dev/null 2>&1 &
echo $! >"$T/live.pid"
: >"$TEHAS_HOME/state/cycles/$1.taken"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-live TEHAS_CYCLE_START_WAIT=2 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index)
t_run "$dispatch"
alive=no
kill -0 "$(cat "$T/live.pid")" 2>/dev/null && alive=yes
free=$(flock -n "$T/home/tehas/state/dispatch.lock" echo free)
kill "$(cat "$T/live.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_eq "$alive" yes
t_eq "$free" free

t_case "while a cycle is running nothing is started"
reset
hold 9
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start)" ""
t_lacks "$(t_calls gh)" "issue edit"
t_run "$dispatch --dry-run"
t_contains "$t_out" "holding: 1 cycles are running (limit 1)"
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

t_case "a cycle that holds its lock but wrote no marker in time is taken as started"
reset
# The holder keeps the cycle's lock for a few seconds with its output detached,
# so the stub returns at once and the test's pipe is not held open.
t_stub fake-late '( exec 9>"$TEHAS_HOME/state/cycles/$1.lock"; flock -n 9; exec sleep 5 ) >/dev/null 2>&1 &
echo $! >"$T/late.pid"'
T_ENV=(TEHAS_CYCLE_START_CMD=fake-late TEHAS_CYCLE_START_WAIT=1 TEHAS_STATUS_CMD=fake-status TEHAS_INDEX_CMD=fake-index)
t_run "$dispatch"
kill "$(cat "$T/late.pid")" 2>/dev/null
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #7 holds its lock but did not say so in 1s; taking it as started"
t_lacks "$(t_calls gh)" "needs-human-attention"

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

t_case "two slots: two issues are started in one poll, in order"
reset
slots 2
ready 7 8
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls fake-start | tr '\n' ' ')" "7 8 "
t_contains "$(t_calls gh)" "issue edit 7 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"
t_contains "$(t_calls gh)" "issue edit 8 --repo acme/widget --add-label tehas:running --remove-label tehas:ready"

t_case "the index is refreshed once, before the first start"
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
default_start

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

# Arguments. A poll starts only from an argument this command knows, and the
# check runs before the instance settings are read.
t_case "--help prints the usage of the command and calls nothing"
reset
t_run "$dispatch --help 2>/dev/null"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_contains "$t_out" "tehas dispatch --dry-run"
t_lacks "$t_out" "set -uo pipefail"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""

t_case "-h is the same as --help"
reset
t_run "$dispatch -h 2>/dev/null"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_eq "$(t_calls gh)" ""

t_case "--help writes to standard output and nothing to standard error"
reset
t_run "$dispatch --help 1>/dev/null"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_case "--help answers on a host with no instance settings yet"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance
t_run "$dispatch --help"
t_eq "$t_rc" 0
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "a mistyped argument starts no poll and creates nothing"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_run "$dispatch --dryrun"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --dryrun"
t_contains "$t_out" "Usage:"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "an unknown argument is reported on standard error, never as a poll"
reset
t_run "$dispatch --dryrun 2>/dev/null"
t_eq "$t_rc" 64
t_lacks "$t_out" "unknown argument"
t_run "$dispatch --dryrun 1>/dev/null"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --dryrun"
t_contains "$t_out" "Usage:"

t_case "an unknown argument is refused before the instance settings are read"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_instance TEHAS_CONTROL_ISSUE=1 TEHAS_ALLOWED_LABELERS=alice
t_run "$dispatch --dryrun"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --dryrun"
t_lacks "$t_out" "TEHAS_REPO is not set"
t_eq "$(t_calls gh)" ""

t_case "an unknown option is refused too, whatever it looks like"
reset
for arg in "-x" "--dry-run=1" "7" "--"; do
  t_run "$dispatch '$arg'"
  t_eq "$t_rc" 64
  t_contains "$t_out" "tehas dispatch: unknown argument: $arg"
done
t_eq "$(t_calls gh)" ""

t_case "an empty argument is an argument, not a poll"
reset
t_run "$dispatch ''"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument:"
t_eq "$(t_calls gh)" ""
t_eq "$(t_calls fake-start)" ""

t_case "two arguments are refused, not combined"
reset
rm -rf "$T/home/tehas/state" "$T/home/tehas/logs"
t_run "$dispatch --dry-run --help"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --help"
t_lacks "$t_out" "would run a cycle"
t_eq "$(t_calls gh)" ""
t_eq "$(find "$T/home/tehas" -maxdepth 1 \( -name state -o -name logs \) | wc -l)" 0

t_case "the same argument twice is refused too"
reset
t_run "$dispatch --dry-run --dry-run"
t_eq "$t_rc" 64
t_contains "$t_out" "unknown argument: --dry-run"
t_eq "$(t_calls gh)" ""

t_case "--help with another argument is still an unknown invocation"
reset
t_run "$dispatch --help --dry-run"
t_eq "$t_rc" 64
t_contains "$t_out" "tehas dispatch: unknown argument: --dry-run"
t_eq "$(t_calls gh)" ""

t_case "no argument and --dry-run still behave as before"
reset
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_contains "$t_out" "would run a cycle for #7"
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$(t_calls fake-start)" "7"

# Outside the tests nothing names the scripts: the dispatcher runs the ones in
# its own directory, whatever PATH holds, and starts a cycle through systemd.
bare_tree() {
  T_ENV=()
  tree=$T/tree
  rm -rf "$tree"
  mkdir -p "$tree/libexec" "$tree/lib"
  cp "$dispatch" "$tree/libexec/dispatch"
  cp "$T_ROOT/lib/config.sh" "$T_ROOT/lib/cycles.sh" "$tree/lib/"
  for name in status index; do cp "$T/bin/fake-$name" "$tree/libexec/$name"; done
  for name in tehas tehasbot tehas-work tehas-status tehas-index; do t_stub "$name"; done
}

t_case "without the test seams it runs the scripts beside it and starts the unit"
reset
bare_tree
t_stub systemctl 'case "$2" in
  show) echo loaded ;;
  start) n=${3#tehas-cycle@}
    : >"$TEHAS_HOME/state/cycles/${n%.service}.taken" ;;
esac'
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl)" "--user show -p LoadState --value tehas-cycle@7.service
--user start tehas-cycle@7.service"
t_contains "$(t_calls fake-index)" "--before-cycle"
t_eq "$(t_calls fake-status | wc -l)" 2
for name in tehas tehasbot tehas-work tehas-status tehas-index; do t_eq "$(t_calls "$name")" ""; done

t_case "a unit systemd cannot load changes no label and starts nothing, exit 1"
reset
bare_tree
t_stub systemctl 'case "$2" in show) echo not-found ;; esac'
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
t_stub systemctl 'case "$2" in show) exit 1 ;; esac'
t_run "$tree/libexec/dispatch"
t_eq "$t_rc" 1
t_contains "$t_out" "ERROR: systemd cannot load tehas-cycle@.service (LoadState: no answer): run install.sh"
t_lacks "$(t_calls gh)" "issue edit"
t_lacks "$(t_calls gh)" "issue comment"
t_lacks "$(t_calls systemctl)" "start"

t_done
