#!/usr/bin/env bash
# Tests for tehasbot dispatch: the workspaces, transcripts and console file a
# cycle leaves that are removed, and cycles that have gone quiet.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

# Kept workspaces. Issue 5's were last used eight days ago, issue 6's today.
# Each issue a case leaves something of is open on GitHub, unless the case
# closes it: what a closed issue left goes at once.
works=$T/home/tehas/work
open_issue() { printf '{"number":%s,"state":"open"}\n' "$1" >"$fx/work-$1.json"; }
closed_issue() { printf '{"number":%s,"state":"closed"}\n' "$1" >"$fx/work-$1.json"; }
kept() { # <issue> <age, as touch -d takes it> [what is beside it]
  local p
  open_issue "$1"
  mkdir -p "$works" "$T/home/tehas/state/cycles"
  : >"$works/.tehas-workspaces"
  for p in "$works/widget-$1" "${@:3}"; do
    mkdir -p "$p"
    touch -d "$2" "$p"
  done
  touch -d "$2" "$T/home/tehas/state/cycles/$1.lock"
}
old() { kept "$1" "8 days ago" "$works/widget-$1.before-retry" "$works/widget-$1.images" "$works/widget-$1.state"; }
left() { find "$works" -mindepth 1 -maxdepth 1 ! -name .tehas-workspaces -printf '%f\n' | sort | tr '\n' ' '; }
# What the poll said it took. One line per issue, so a case can name the issue
# it means without matching the timestamps of the log.
swept() { grep -o 'what #[0-9]* left on this host' <<<"$t_out" | tr '\n' ' '; }
# The three a cycle leaves outside its workspace: the agents' transcripts of the
# cycle and of its planning session, and the console the cycle wrote.
runs=$T/home/.pi/agent/ensemble-runs
live=$T/home/tehas/live
cycles=$T/home/tehas/state/cycles
# The issues whose leftovers the cases made. A poll writes files into live/ of
# its own, so reset cannot clear that directory once and for all: each case
# takes away what the cases before it made, as its own poll has run by then, and
# its assertions read only what the case itself made.
made=()
unsaid() {
  local n f
  mkdir -p "$live"
  for n in "${made[@]}"; do
    for f in "$runs/issue-$n" "$runs/plan-$n" "$live/console-$n.txt"; do
      rm -rf "$f"
    done
  done
  made=()
}
# A clean host, and no name left by an earlier case: the first thing a case does.
reset_said() {
  reset
  unsaid
}
said() { # <issue> <age, as touch -d takes it>
  open_issue "$1"
  mkdir -p "$runs/issue-$1" "$runs/plan-$1" "$live"
  printf 'transcript\n' >"$runs/issue-$1/a-1-coder.json"
  printf 'transcript\n' >"$runs/plan-$1/p-1-planner.json"
  printf 'console line\n' >"$live/console-$1.txt"
  touch -d "$2" "$runs/issue-$1" "$runs/plan-$1" "$live/console-$1.txt"
  made+=("$1")
}
# What <issue> still has of those three on the host, by path so a case can say
# the whole list at once.
left_said() { # <issue>
  local n=$1 out=""
  for f in "$runs/issue-$n" "$runs/plan-$n" "$live/console-$n.txt"; do
    [ -e "$f" ] && out="$out $f"
  done
  printf '%s\n' "${out# }"
}

t_case "a workspace no cycle has used for seven days goes, with what is beside it, its transcripts and its console, and that is logged"
reset_said
rm -rf "$works"
old 5
said 5 "8 days ago"
kept 6 "1 day ago" "$works/widget-6.before-retry"
said 6 "8 days ago"
mkdir -p "$works/widget-5x" "$works/other-5"
touch -d "8 days ago" "$works/widget-5x" "$works/other-5"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "other-5 widget-5x widget-6 widget-6.before-retry "
t_eq "$(left_said 5)" ""
t_eq "$(left_said 6)" "$runs/issue-6 $runs/plan-6 $live/console-6.txt"
t_contains "$t_out" "removed what #5 left on this host: no cycle has run on it for 7 days"
t_eq "$(swept)" "what #5 left on this host "

t_case "a workspace whose issue's lock is held is kept, however old, and so are its transcripts and its console"
reset_said
rm -rf "$works"
old 5
said 5 "8 days ago"
hold 5
touch -d "8 days ago" "$works/widget-5" "$T/home/tehas/state/cycles/5.lock"
t_run "$dispatch"
release
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "
t_eq "$(left_said 5)" "$runs/issue-5 $runs/plan-5 $live/console-5.txt"
t_eq "$(swept)" ""

# An orphan: the workspace is gone, the three beside it are not.
t_case "an issue's transcripts and console with no workspace left go on their own, and that is logged"
reset_said
rm -rf "$works"
kept 5 "8 days ago"
rm -rf "$works/widget-5" "$works/widget-5.state" "$works/widget-5.before-retry" "$works/widget-5.images"
said 5 "8 days ago"
said 51 "8 days ago"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(swept)" "what #5 left on this host what #51 left on this host "
t_eq "$(left_said 5)" ""
t_eq "$(left_said 51)" ""
t_eq "$(find "$works" -mindepth 1 -maxdepth 1 ! -name .tehas-workspaces | wc -l)" 0
t_contains "$t_out" "removed what #5 left on this host: no cycle has run on it for 7 days"

# The same three, touched too recently: the age rule reads them as any other.
t_case "transcripts and a console written within seven days are kept"
reset_said
rm -rf "$works"
said 5 "8 days ago"
said 6 "1 day ago"
t_run "$dispatch"
t_eq "$(left_said 5)" ""
t_eq "$(left_said 6)" "$runs/issue-6 $runs/plan-6 $live/console-6.txt"
t_eq "$(swept)" "what #5 left on this host "

t_case "what a closed issue left goes at once, however young, and an open one's is kept"
reset_said
rm -rf "$works"
kept 5 "1 hour ago" "$works/widget-5.before-retry" "$works/widget-5.state"
said 5 "1 hour ago"
closed_issue 5
kept 6 "1 hour ago"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "widget-6 "
t_eq "$(left_said 5)" ""
t_contains "$t_out" "removed what #5 left on this host: its issue is closed"
t_eq "$(swept)" "what #5 left on this host "

t_case "a closed issue whose lock is held keeps what it left"
reset_said
rm -rf "$works"
kept 5 "1 hour ago"
closed_issue 5
hold 5
t_run "$dispatch"
release
t_eq "$(left)" "widget-5 "

t_case "TEHAS_WORKSPACE_KEEP_DAYS sets the days an open issue's workspace is kept"
reset_said
t_instance "${T_MINIMAL[@]}" TEHAS_TICKET_CONCURRENCY=1 TEHAS_WORKSPACE_KEEP_DAYS=2
rm -rf "$works"
kept 5 "3 days ago"
kept 6 "1 day ago"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "widget-6 "
t_contains "$t_out" "removed what #5 left on this host: no cycle has run on it for 2 days"

# The dated directories are the runner's, not tehas's: no rule of the sweep
# names them and a poll leaves them whole.
t_case "a dated directory of ensemble-runs is kept, however old, and says nothing"
reset_said
rm -rf "$works"
mkdir -p "$runs/2026-10-08" "$runs/2026-10-11"
printf 't\n' >"$runs/2026-10-08/handoff:ops:8.json"
touch -d "30 days ago" "$runs/2026-10-08" "$runs/2026-10-11"
said 5 "8 days ago"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(find "$runs" -maxdepth 1 -name '2026-*' | wc -l)" "2"
t_eq "$(cat "$runs/2026-10-08/handoff:ops:8.json")" "t"
t_eq "$(left_said 5)" ""
t_eq "$(swept)" "what #5 left on this host "

# A poll takes three issues at most, and the orphans count toward that with the
# rest: the four oldest issues leave one of them for the next poll. The number
# is read low here, so the four ready issues of the stub's queue cannot start in
# the slots the orphans are meant to hold back.
t_case "orphans count toward the three issues a poll takes"
reset_said
# The queue is empty and one slot is free, so this case is about the sweep
# alone: no issue of its own starts beside it, and no start writes a name.
slots 1
echo '[]' >"$fx/ready.json"
echo '[]' >"$fx/marked.json"
rm -rf "$works"
for n in 2 3 4 5; do said "$n" "8 days ago"; done
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left_said 2)" ""
t_eq "$(left_said 3)" ""
t_eq "$(left_said 4)" ""
t_eq "$(left_said 5)" "$runs/issue-5 $runs/plan-5 $live/console-5.txt"
t_run "$dispatch"
t_eq "$(left_said 5)" ""

# The marker is what keeps a workspace from going; the three beside an issue are
# not workspaces, so an unmarked directory loses them and keeps its clones.
t_case "a directory tehas work did not mark loses an issue's transcripts and console and keeps its workspaces"
reset_said
rm -rf "$works"
old 5
said 5 "8 days ago"
rm "$works/.tehas-workspaces"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "
t_eq "$(left_said 5)" ""
t_contains "$t_out" "removed what #5 left on this host: no cycle has run on it for 7 days"

t_case "a workspace moved aside recently keeps the issue's workspaces"
reset
rm -rf "$works"
old 5
touch "$works/widget-5.before-retry"
t_run "$dispatch"
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "

# The same refusal tehas stop makes of a lock that is no lock file: a name of
# that kind says nothing about the issue and taking it would write through it.
# It is flock-able and no cycle holds it, so the lock itself keeps nothing: the
# poll has to notice the type, and it says so rather than removing in silence.
t_case "a lock that is no lock file keeps the whole issue, and what it points at is not touched"
reset_said
rm -rf "$works"
old 5
said 5 "8 days ago"
mkdir -p "$cycles"
rm -f "$cycles/5.lock"
printf 'a person reads this\n' >"$T/target"
ln -s "$T/target" "$cycles/5.lock"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(cat "$T/target")" "a person reads this"
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "
t_eq "$(left_said 5)" "$runs/issue-5 $runs/plan-5 $live/console-5.txt"
t_eq "$(swept)" ""
t_contains "$t_out" "the lock of #5 is no lock file, so nothing of it was removed"

# A name the poll cannot date is not a name it proved old, so the issue is kept
# for the next poll, as tehasbot clean keeps what it cannot date. The poll asks
# find of each name on its own: one find over the whole set prints nothing for a
# name it could not read and ends 0 all the same, so a name it cannot read and a
# name it read as old come to one answer. The name that cannot be dated here sits
# behind a directory of no rights, which is the shape that prints nothing at all
# to that one find. The case asks the poll twice: once while the name cannot be
# read, and once after it can.
t_case "an issue the poll cannot date is kept, and the whole of it goes once it can"
reset_said
rm -rf "$works"
mkdir -p "$works" "$cycles"
: >"$works/.tehas-workspaces"
mkdir -p "$works/undatable"
mkdir -p "$works/widget-5" "$works/widget-5.before-retry" "$works/widget-5.images" "$works/widget-5.state"
said 5 "8 days ago"
for n in 2 3 4; do said "$n" "8 days ago"; done
touch -d "8 days ago" "$cycles/5.lock"
mv "$works/widget-5" "$works/widget-5.before-retry" "$works/widget-5.images" "$works/widget-5.state" "$works/undatable/"
chmod 000 "$works/undatable"
# The rule needs two things, and the case shows both: the poll could not date
# the name of its own accord, and one find over all of the issue's names printed
# nothing, which is the answer that reads as aged out. Without them a poll keeps
# an issue for a reason the case did not make.
if [ -n "$(find "$cycles/5.lock" "$works/widget-5" "$works/widget-5.before-retry" \
  "$works/widget-5.images" "$works/widget-5.state" \
  "$runs/issue-5" "$runs/plan-5" "$live/console-5.txt" -maxdepth 0 -newermt "-7 days" 2>/dev/null)" ]; then
  chmod 755 "$works/undatable"
  t_bad "one find over the issue's names dated something, so the case does not show the rule"
# The setup is only real when this user cannot read the directory either: an
# unreadable tree the test reads through is no unreadable tree, and a case that
# passes for no reason is worse than one that fails.
elif [ -e "$works/undatable/widget-5" ] || [ -L "$works/undatable/widget-5" ]; then
  chmod 755 "$works/undatable"
  t_bad "the poll can read the directory the test gave it 000, so it cannot be denied a date"
elif find "$works/undatable/widget-5" -maxdepth 0 -newermt "-7 days" >/dev/null 2>&1;
then
  chmod 755 "$works/undatable"
  t_bad "find dates a name the test cannot read, so the age rule cannot be shown failing"
else
  t_run "$dispatch"
  t_eq "$t_rc" 0
  # #5 is kept and costs the poll nothing: the three it took are the others.
  t_eq "$(swept)" "what #2 left on this host what #3 left on this host what #4 left on this host "
  t_eq "$(left_said 5)" "$runs/issue-5 $runs/plan-5 $live/console-5.txt"
  chmod 755 "$works/undatable"
  t_run "$dispatch"
  t_eq "$(left_said 5)" ""
  t_contains "$t_out" "removed what #5 left on this host: no cycle has run on it for 7 days"
fi

# The age rule reads the names of the issue one by one, so a date read off the
# lock is no answer for the console beside it: one name touched today keeps the
# whole set for another 7 days.
t_case "an issue is kept while one name of it is young, whatever the lock says"
reset_said
rm -rf "$works"
old 5
said 5 "8 days ago"
touch "$live/console-5.txt"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "
t_eq "$(left_said 5)" "$runs/issue-5 $runs/plan-5 $live/console-5.txt"
t_eq "$(swept)" ""

# The refusal above costs no candidate of another issue: the issue whose lock is
# no lock file is passed over before the three of a poll are spent, so the
# others of the same poll still go.
t_case "an issue whose lock is no lock file does not spend one of the three a poll takes"
reset_said
rm -rf "$works"
for n in 2 3 4 5; do said "$n" "8 days ago"; done
mkdir -p "$cycles"
rm -f "$cycles/5.lock"
printf 'a person reads this\n' >"$T/target"
ln -s "$T/target" "$cycles/5.lock"
t_run "$dispatch"
t_eq "$t_rc" 0
t_eq "$(swept)" "what #2 left on this host what #3 left on this host what #4 left on this host "
t_eq "$(cat "$T/target")" "a person reads this"
t_eq "$(left_said 5)" "$runs/issue-5 $runs/plan-5 $live/console-5.txt"
rm -f "$cycles/5.lock"
t_run "$dispatch"
t_eq "$(swept)" "what #5 left on this host "
rm -rf "$works"

t_case "a dry run says what would go and removes nothing"
reset_said
rm -rf "$works"
old 5
said 5 "8 days ago"
t_run "$dispatch --dry-run"
t_contains "$t_out" "would remove what #5 left on this host: no cycle has run on it for 7 days"
t_eq "$(left)" "widget-5 widget-5.before-retry widget-5.images widget-5.state "
t_eq "$(left_said 5)" "$runs/issue-5 $runs/plan-5 $live/console-5.txt"

t_case "a poll removes the workspaces of three issues at most"
reset_said
rm -rf "$works"
for n in 2 3 4 5; do kept "$n" "8 days ago"; done
t_run "$dispatch"
t_eq "$(left)" "widget-5 "
t_run "$dispatch"
t_eq "$(left)" ""
rm -rf "$works"

# #9 was last heard from <age> seconds ago: its heartbeat is on record then.
heard_at() { # <age>
  echo "$(($(date +%s) - $1))" >"$T/heard-9"
  t_at "$(cat "$T/heard-9")" cycle-heartbeat 9 --running --status running --step develop >/dev/null ||
    t_bad "the store did not take the heartbeat of #9"
}
# A cycle on #9 whose heartbeat is <age> seconds old, and that took its lock
# <taken> seconds ago: its start on record at that time, under the lock hold
# took.
quiet_cycle() { # <age> <taken>
  hold 9
  t_at "$(($(date +%s) - $2))" cycle-start 9 --by-hand >/dev/null ||
    t_bad "the store did not start the record of #9"
  heard_at "$1"
}
# The issues with a quiet episode on record.
notes() { t_row cycle-show 9 -- 'if .cycle.quiet == null then "" else "9 " end'; }
# How many times the poll has asked #9's unit to stop in the episode on record.
note_asks() {
  t_row cycle-show 9 -- '.cycle | if .quiet != null and (.stop_asked_at // -1) >= .quiet.at
    then .stop_said_times else 0 end'
}
# The episode of #9 as it stands was first seen a day ago, so the cases can
# move the time of the last ask back without starting a new episode.
seen_long_ago() {
  t_at "$(($(date +%s) - 86400))" cycle-note 9 --kind quiet \
    --said-for "$(cat "$T/heard-9")" >/dev/null ||
    t_bad "the store did not take the episode of #9"
}
# Time passes: the last ask or line about #9 was <seconds> ago. The count of
# asks stays as it is. A case that means "not yet" stays 10 s short of the
# wait, as the poll reads its clock a moment after this.
ago() { # <seconds>
  t_at "$(($(date +%s) - $1))" cycle-stop-asked 9 --said-only >/dev/null ||
    t_bad "the store did not move the last ask of #9"
}

t_case "a cycle that holds its lock and has not been heard from for 20 minutes is reported and stopped, once"
reset
t_stub fake-stop '[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/stop.fd8"'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #9 holds its lock but has not been heard from since "
t_contains "$t_out" "more than 1200s: stopping tehas-cycle@9.service"
t_eq "$(t_calls fake-stop)" "9"
t_eq "$(find "$T/calls/stop.fd8" 2>/dev/null | wc -l)" 0
t_eq "$(notes)" "9 "
t_eq "$(note_asks)" "1"
t_run "$dispatch"
t_lacks "$t_out" "#9 holds its lock"
t_eq "$(t_calls fake-stop)" "9"
t_eq "$(notes)" "9 "
t_eq "$(note_asks)" "1"
release

t_case "a stop that failed is said, recorded, and asked again once the wait has passed"
reset
t_stub fake-stop '[ ! -e "/proc/$$/fd/8" ] || : >"$T/calls/stop.fd8"; exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
seen_long_ago
t_run "$dispatch"
t_contains "$t_out" "WARN: could not stop tehas-cycle@9.service (exit 1)"
t_contains "$t_out" "it holds its slot, and the poll asks again"
t_eq "$(find "$T/calls/stop.fd8" 2>/dev/null | wc -l)" 0
t_eq "$(note_asks)" "1"
# The wait for the next ask has not passed: this poll says nothing.
ago 590
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 "
# The wait has passed: the same episode is said and the stop asked again.
ago 601
t_run "$dispatch"
t_contains "$t_out" "the poll asks tehas-cycle@9.service to stop again"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 "
t_eq "$(note_asks)" "2"
release

t_case "a stop that never works is asked six times, 10 minutes apart, then said once an hour and left to a person"
reset
t_stub fake-stop 'exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
seen_long_ago
t_run "$dispatch"
t_contains "$t_out" "stopping tehas-cycle@9.service"
for _ in 2 3 4 5 6; do
  ago 601
  t_run "$dispatch"
  t_contains "$t_out" "the poll asks tehas-cycle@9.service to stop again"
done
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 9 9 9 9 "
t_eq "$(note_asks)" "6"
# The sixth ask waits for its 10 minutes like the others, and then the poll
# says the episode is a person's.
ago 590
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
ago 601
t_run "$dispatch"
t_contains "$t_out" "asked 6 times to stop tehas-cycle@9.service and asks no more"
t_contains "$t_out" "systemctl --user stop tehas-cycle@9.service"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 9 9 9 9 "
# The first line is on record as one more: that tells it from the sixth ask.
t_eq "$(note_asks)" "7"
# That line is said again once an hour, and the stop is never asked again.
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
ago 3590
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
ago 3601
t_run "$dispatch"
t_contains "$t_out" "asked 6 times to stop tehas-cycle@9.service and asks no more"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 9 9 9 9 "
ago 3601
t_run "$dispatch"
t_contains "$t_out" "asked 6 times to stop tehas-cycle@9.service and asks no more"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 9 9 9 9 "
release

t_done
