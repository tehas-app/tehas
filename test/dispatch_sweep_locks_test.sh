#!/usr/bin/env bash
# Tests for tehasbot dispatch: the lock files of cycles that ended, and the
# store a restored backup leaves.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/dispatch_lib.sh
. "$(dirname "$0")/dispatch_lib.sh" || exit 1

cycles=$T/home/tehas/state/cycles
# A cycle of #<n> that ended <days> days ago, its lock file left.
ended_days_ago() { # <n> <days>
  hold "$1"
  t_at "$(($(date +%s) - $2 * 86400 - 60))" cycle-start "$1" --by-hand >/dev/null ||
    t_bad "the store did not start the record of #$1"
  release
  t_at "$(($(date +%s) - $2 * 86400))" cycle-end "$1" --attempt 1 --status merged >/dev/null ||
    t_bad "the store did not end the record of #$1"
  # As old as its cycle: a lock touched in the last minute may have a waiter.
  touch -d "$2 days ago" "$T/home/tehas/state/cycles/$1.lock"
}

t_case "the lock of an issue nothing has used for 30 days goes, and one used lately stays"
reset
ended_days_ago 31 40
ended_days_ago 32 10
t_run "$dispatch --dry-run"
t_contains "$t_out" "would remove the lock of #31: unused for 30 days"
t_eq "$(find "$T/home/tehas/state/cycles" -name '3[12].lock' -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" "31.lock 32.lock "
t_run "$dispatch"
t_contains "$t_out" "removed the lock of #31: unused for 30 days"
t_lacks "$t_out" "#32: unused"
t_eq "$([ -e "$T/home/tehas/state/cycles/31.lock" ] && echo kept)" ""
t_eq "$([ -e "$T/home/tehas/state/cycles/32.lock" ] && echo kept)" kept

t_case "a backup restored into a fresh home is a store a poll runs on"
reset
ended_days_ago 33 1
TEHAS_HOME=$T/home/tehas "$T_STORE" backup >/dev/null || t_bad "the store made no backup"
copy=$(find "$T/home/tehas/state/backup" -name 'tehas-[0-9]*.db' | head -n 1)
cp "$copy" "$T/restored.db"
reset
cp "$T/restored.db" "$T/home/tehas/state/tehas.db"
rm -f "$T/home/tehas/state/tehas.db-wal" "$T/home/tehas/state/tehas.db-shm"
chmod 600 "$T/home/tehas/state/tehas.db"
t_eq "$(TEHAS_HOME=$T/home/tehas "$T_STORE" check | jq -r .integrity)" ok
t_eq "$(t_row cycle-show 33 -- .cycle.end_reason)" merged
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0

# The poll asks the store once which locks are held, so a host with a long
# history of issues probes no more than one with none. A probe never runs
# `true`, which flock would look for through every directory of PATH.
t_case "a thousand old lock files cost the poll no probe each, and the two held ones still count"
reset
mkdir -p "$cycles"
for n in $(seq 1001 2000); do : >"$cycles/$n.lock"; done
hold 7
hold 8
t_stub flock "exec $(command -pv flock) \"\$@\""
t_stub true
t_run "$dispatch --dry-run"
t_eq "$t_rc" 0
t_eq "$(t_calls true)" ""
probes=$(t_calls flock | wc -l)
[ "$probes" -lt 20 ] || t_bad "the poll ran flock $probes times over 1,002 lock files"
release
rm -f "$T/bin/flock" "$T/bin/true"


t_done

