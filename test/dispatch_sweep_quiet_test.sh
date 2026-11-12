#!/usr/bin/env bash
# Tests for tehasbot dispatch: a cycle that holds its lock and has gone quiet,
# and what the poll asks of its unit.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

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

t_case "an ask is recorded before the stop, so a slow stop does not put off the next ask"
reset
t_stub fake-stop 'sleep 3; date +%s >"$T/calls/stop.done"'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
seen_long_ago
t_run "$dispatch"
t_contains "$t_out" "stopping tehas-cycle@9.service"
asked=$(t_row cycle-show 9 -- .cycle.stop_asked_at)
t_eq "$(( $(cat "$T/calls/stop.done") - asked >= 3 ))" 1
# The record is dated a moment after the poll's clock: the poll that comes
# quiet_retry after the ask, by its own clock, asks again.
ago 599
t_run "$dispatch"
t_contains "$t_out" "the poll asks tehas-cycle@9.service to stop again"
t_eq "$(note_asks)" "2"
release

t_case "a quiet cycle that is heard from again is left alone"
reset
t_stub fake-stop 'exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
seen_long_ago
t_run "$dispatch"
ago 601
t_run "$dispatch"
t_eq "$(note_asks)" "2"
# The stop was asked, and the cycle went on with its heartbeats.
heard_at 20
ago 3601
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 "
release

# A heartbeat only moves forward, so the new episode is a cycle of its own.
t_case "a quiet cycle heard from since its episode is a new episode, whose asks count from one"
reset
t_stub fake-stop 'exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
seen_long_ago
t_run "$dispatch"
ago 601
t_run "$dispatch"
t_eq "$(note_asks)" "2"
# The store dates in whole seconds, and an ask in the second the new episode
# begins counts as the episode's own. Polls are minutes apart; these three run
# back to back, so the last ask goes back a few seconds first.
ago 5
# Quiet again, since a later time: the first ask of a new episode.
heard_at 1250
t_run "$dispatch"
t_contains "$t_out" "more than 1200s: stopping tehas-cycle@9.service"
t_lacks "$t_out" "to stop again"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 9 "
t_eq "$(note_asks)" "1"
release

t_case "a quiet cycle whose episode the store cannot take is not stopped, and is asked once it can"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_store_answers cycle-note 1 'store: busy #9: the database is locked' 1
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the cycle for #9 holds its lock but has not been heard from since "
t_contains "$t_out" "the state store could not record it (busy), so the poll asks nothing this time"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(notes)" ""
t_run "$dispatch"
t_contains "$t_out" "stopping tehas-cycle@9.service"
t_eq "$(t_calls fake-stop)" "9"
t_eq "$(note_asks)" "1"
release

t_case "a store that cannot be used ends the poll and stops no quiet cycle"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_store_answers cycle-note 78 'store: corrupt: the database disk image is malformed'
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store cannot be used: store: corrupt: the database disk image is malformed"
t_lacks "$t_out" "so the poll asks nothing this time"
t_eq "$(t_calls fake-stop)" ""
t_store_answers cycle-note 64 'store: usage cycle-note: --said-for'
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store cannot be used: store: usage cycle-note"
t_eq "$(t_calls fake-stop)" ""
# The episode is taken, and the ask cannot be recorded: nothing is stopped.
t_store_answers cycle-stop-asked 78 'store: corrupt: the database disk image is malformed'
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "ERROR: the state store cannot be used: store: corrupt"
t_eq "$(t_calls fake-stop)" ""
release

t_case "a store that cannot be used when the poll says a cycle needs a person ends the poll"
reset
t_stub fake-stop 'exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
seen_long_ago
for _ in 1 2 3 4 5 6; do
  ago 601
  t_run "$dispatch"
done
t_eq "$(note_asks)" "6"
ago 601
t_store_answers cycle-stop-asked 78 'store: corrupt: the database disk image is malformed'
t_run "$dispatch"
t_eq "$t_rc" 78
t_contains "$t_out" "asks no more"
t_contains "$t_out" "ERROR: the state store cannot be used: store: corrupt"
release

t_case "a line the store does not record is said again on the next poll"
reset
t_stub fake-stop 'exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
seen_long_ago
for _ in 1 2 3 4 5 6; do
  ago 601
  t_run "$dispatch"
done
ago 601
t_store_answers cycle-stop-asked 1 'store: busy #9: the database is locked' 1
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "asks no more"
t_contains "$t_out" "WARN: the state store did not record that the poll asks no more of tehas-cycle@9.service (busy)"
t_eq "$(note_asks)" "6"
t_run "$dispatch"
t_contains "$t_out" "asked 6 times to stop tehas-cycle@9.service and asks no more"
t_lacks "$t_out" "did not record"
t_eq "$(note_asks)" "7"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 9 9 9 9 "
release

t_case "a quiet cycle whose record does not read as one is asked nothing"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_store_answers cycle-note 0 '{"api":1,"verb":"cycle-note","ok":true,"said":true,"episode_at":1,"times":"many","stop_asked_at":null}'
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "the state store's record of it does not read as one, so the poll asks nothing this time"
t_eq "$(t_calls fake-stop)" ""
release

t_case "a stop the store does not record is asked again on the next poll"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_store_answers cycle-stop-asked 1 'store: busy #9: the database is locked' 1
t_run "$dispatch"
t_contains "$t_out" "stopping tehas-cycle@9.service"
t_contains "$t_out" "WARN: the state store did not record the stop of tehas-cycle@9.service (busy): the poll asks again"
t_eq "$(t_calls fake-stop)" "9"
t_run "$dispatch"
t_contains "$t_out" "stopping tehas-cycle@9.service"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 "
t_eq "$(note_asks)" "1"
release

t_case "a stuck cycle whose last-heard time cannot be read says nothing and keeps its note"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_eq "$(notes)" "9 "
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 "
# The store answers with no cycle on record: nothing is known about this
# cycle.
t_store_answers cycle-show 0 '{"api":1,"verb":"cycle-show","ok":true,"probe_rc":0,"cycles":[{"issue":9,"lock":{"state":"held","probe_rc":1,"file":true,"seen_at":1},"cycle":null}]}'
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(notes)" "9 "
t_eq "$(note_asks)" "1"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 "
release

t_case "a quiet cycle whose record has ended is not stopped by the poll, which says so for a person"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
TEHAS_HOME=$T/home/tehas "$T_STORE" cycle-end 9 --attempt "$(t_row cycle-show 9 -- .cycle.attempt)" --refused >/dev/null
t_run "$dispatch"
t_eq "$t_rc" 0
t_lacks "$t_out" "stopping tehas-cycle@9.service"
t_contains "$t_out" "WARN: the cycle for #9 holds its lock but has not been heard from since "
t_contains "$t_out" "and the state store has no open record of it, so the poll does not stop it: if it stays, a person can, with tehas stop 9 or systemctl --user stop tehas-cycle@9.service"
t_eq "$(t_calls fake-stop)" ""
t_run "$dispatch --dry-run"
t_lacks "$t_out" "would stop"
t_contains "$t_out" "and the state store has no open record of it, so the poll does not stop it"
t_eq "$(t_calls fake-stop)" ""
release

t_case "a quiet lock whose record has ended is not stopped by the poll, which says so for a person"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
hold 9
t_at "$(($(date +%s) - 3600))" cycle-start 9 --by-hand >/dev/null || t_bad "the store did not start the record of #9"
heard_at 1300
TEHAS_HOME=$T/home/tehas "$T_STORE" cycle-end 9 --attempt "$(t_row cycle-show 9 -- .cycle.attempt)" --refused >/dev/null
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "and the state store has no open record of it, so the poll does not stop it"
t_eq "$(t_calls fake-stop)" ""
t_run "$dispatch --dry-run"
t_lacks "$t_out" "would stop"
t_contains "$t_out" "and the state store has no open record of it"
release

t_case "a cycle whose record ends after the poll listed it is not stopped, and that is said"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_store_answers cycle-note 1 'store: nocycle #9: no cycle on record'
t_run "$dispatch"
t_eq "$t_rc" 0
t_contains "$t_out" "and the state store has no open record of it, so the poll does not stop it"
t_lacks "$t_out" "could not record it"
t_eq "$(t_calls fake-stop)" ""
release

t_case "a cycle heard from lately, or one that took its lock lately, is left alone"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 20 3600
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
release
# A live file left by the issue's last cycle, and a new one still starting.
quiet_cycle 86400 60
t_run "$dispatch"
t_lacks "$t_out" "holds its lock but"
t_eq "$(t_calls fake-stop)" ""
release

t_case "a dry run says what the poll would do about a quiet cycle, and stops nothing"
reset
t_stub fake-stop 'exit 1'
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service: the cycle for #9 holds its lock but has not been heard from since "
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service"
t_eq "$(t_calls fake-stop)" ""
t_eq "$(notes)" ""
seen_long_ago
t_run "$dispatch"
t_eq "$(t_calls fake-stop)" "9"
# Inside the wait the poll says nothing, and so does the dry run.
t_run "$dispatch --dry-run"
t_lacks "$t_out" "holds its lock but"
ago 601
t_run "$dispatch --dry-run"
t_contains "$t_out" "would ask tehas-cycle@9.service to stop again: the cycle for #9 holds its lock"
t_lacks "$t_out" "would stop"
for _ in 2 3 4 5 6; do
  ago 601
  t_run "$dispatch"
done
t_eq "$(note_asks)" "6"
ago 601
t_run "$dispatch --dry-run"
t_contains "$t_out" "would say that tehas-cycle@9.service needs a person and ask no more"
t_lacks "$t_out" "would ask"
t_lacks "$t_out" "would stop"
t_eq "$(note_asks)" "6"
t_eq "$(t_calls fake-stop | tr '\n' ' ')" "9 9 9 9 9 9 "
# A cycle heard from since the episode on record: a new one, which the poll
# would stop.
heard_at 1250
t_run "$dispatch --dry-run"
t_contains "$t_out" "would stop tehas-cycle@9.service"
release

t_case "the issue's next cycle starts with no quiet episode on record"
reset
t_stub fake-stop
T_ENV+=(TEHAS_CYCLE_STOP_CMD=fake-stop)
quiet_cycle 1300 3600
t_run "$dispatch"
t_eq "$(notes)" "9 "
# The unit stopped, and the issue's next cycle took the lock.
release
quiet_cycle 20 20
t_eq "$(notes)" ""
t_run "$dispatch"
t_eq "$(notes)" ""
t_eq "$(t_calls fake-stop)" "9"
release


t_done
