#!/usr/bin/env bash
# Tests for the contract of the --json objects of tehas. One row of `surfaces`
# is one object: api/<surface>.schema.json says what it holds, and the samples
# beside it, api/<surface>.<name>.json, are what the code prints for it. A
# program that reads an object copies the samples into its own tests, so a
# change here is a change for it.
#
# The diff, the schema check, the field-path walk and the rewrite are written
# once and loop over the table, so a later surface is one more row. See
# test/status_contract_test.sh for one surface's contract written out in full.
#
# To write the samples again after a change to an object:
#   TEHAS_SAMPLES_WRITE=1 test/json_contract_test.sh

# Read before test/lib.sh is sourced: the sweep it makes of the environment
# takes every TEHAS_ name the settings hold no value for, this with them.
samples_write=${TEHAS_SAMPLES_WRITE:-}

# shellcheck disable=SC2016  # the jq programs are matched as text
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS+=(TEHAS_CONSOLE="$T_CONSOLE")
t_stub gh

api=$T_ROOT/api

# The table. One line starts a surface with `surface` and its name; every line
# after it belongs to that surface: `sample` and a name is one of its samples,
# api/<surface>.<name>.json, and `check`, the jq that makes an object out of
# api/<surface>.sample.json, and the line the check must report it with, are
# one way the schema check must find an object wrong. `required`, the fields
# every object of the surface must hold, is the whole of what `api/<surface>
# .schema.json` must name in its `required`. A later surface is more lines of
# the same four kinds.
surfaces=(
  'surface	limit'
  'sample	sample'
  'sample	expired'
  'sample	empty'
  'sample	unreadable'
  # The state each sample above prints, in the same order: four states of the
  # enum, one printed by one sample. A state of the enum that no sample prints
  # is refused below, so a row cannot be dropped and the schema cannot name a
  # state the command never answers with.
  'state	ok'
  'state	expired'
  'state	none'
  'state	unreadable'
  'check	del(.state) | .zz = 1 | .cycles = "1"	.: state is missing'
  'check	del(.state) | .zz = 1 | .cycles = "1"	.zz: not in the schema'
  'check	del(.state) | .zz = 1 | .cycles = "1"	.cycles: string, not integer'
  # The bounds of the schema: field, the jq that gives it the least value the
  # schema lets through, the jq that gives it one below or beyond that, and the
  # word the check must answer with. Each jq sets that one field of the sample
  # and leaves every other field of it alone, so the object the check is asked
  # about is still an object of the surface. The keyword case below proves only
  # that a keyword is one the check knows; these prove a bound the schema states
  # is a bound the check keeps, so a bound no reading of the command could ever
  # break is not left in the file to be read as its contract.
  # of the surface, so the reading is a limit of the shape the samples have and
  # not an object of another surface with the field missing. Two hundred
  # characters is the longest reason the command keeps (console/core/src/limit
  # .rs), so the line above is a reading the schema lets through and the line
  # below, one character longer, is the bound.
  'bound	cycles	.cycles = 0	.cycles = -1	under 0'
  'bound	until	.until = "2026-10-17T15:00:60Z"	.until = "2026-10-17T15:00:61Z"	does not match'
  'bound	reason	.reason = "01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789"	.reason = "012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567891"	longer than 200'
  'required	["api","state"]'

  'surface	drain'
  'sample	sample'
  'sample	running'
  # The path the state of a sample is read at, the whole enum the surface's
  # schema holds there, and the surface that owns that enum: the words of the
  # line after its kind, the path in the form jq takes a path, with no dot before
  # its first field. A surface that prints its state itself gives no line at all
  # and lists the enum with its `state` lines. A surface that carries the limit
  # of another gives `limit.state`, which is at home in api/limit.schema.json and
  # not at the top level here, and the name of the surface that prints each state
  # of its enum in its own samples - so the states are not asked of samples that
  # could never print them, and are not left out of the contract either. The
  # enum itself is read from the schema and never written here: a list in the
  # table could disagree with the file and be agreed with.
  'state_path	limit.state	limit'
  'state	ok'
  'state	ok'
  'check	del(.drained) | .zz = 1 | .running = "1" | del(.limit.state)	.: drained is missing'
  'check	del(.drained) | .zz = 1 | .running = "1" | del(.limit.state)	.zz: not in the schema'
  'check	del(.drained) | .zz = 1 | .running = "1" | del(.limit.state)	.running: string, not array'
  'check	del(.drained) | .zz = 1 | .running = "1" | del(.limit.state)	.limit: state is missing'
  # The bounds of the schema, as the `limit` surface states them above: the
  # object the check is asked about is `api/drain.sample.json` with the one
  # field set, so every other field of the sample is left as the command
  # printed it, and the object is still an object of the surface. The word the
  # check must answer with is the whole of the words after the path of the
  # field, as the check prints them: no line of the table and no case below
  # cuts a phrase to a part of it, since a part of a line is in every line.
  'bound	drained	.drained = false	.drained = "true"	string, not boolean'
  'bound	running	.running = [0]	.running = [1.5]	number, not integer'
  'bound	limit.cycles	.limit.cycles = 0	.limit.cycles = -1	-1 is under 0'
  'bound	limit.plan	.limit.plan = true	.limit.plan = "on"	string, not boolean'
  'bound	limit.until	.limit.until = "2026-10-17T15:00:60Z"	.limit.until = "2026-10-17T15:00:61Z"	"2026-10-17T15:00:61Z" does not match'
  'bound	limit.reason	.limit.reason = "01234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789"	.limit.reason = "012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567890123456789012345678901234567891"	longer than 200'
  'required	["api","drained","running","limit"]'
  # The object `tehas freeze <issue> --json` and `tehas thaw <issue> --json`
  # print for the same issue: the one object of console/src/freeze.rs, read from
  # the three files of the cycle under state/cycles. The four samples are the
  # four readings of those files a person reaches: no cycle running, a freeze
  # asked of a running cycle, the watch having paused its container, and a thaw
  # asked of that one, whose `frozen` stays true till the watch unpauses it. The
  # surface has no state, so it has no `state` lines and no enum to compare them
  # with: the samples are still held to the schema, to the walk of its fields and
  # to what the two commands print.
  'surface	freeze'
  'sample	idle'
  'sample	asked'
  'sample	frozen'
  'sample	thaw'
  # The one object of the surface, bent one way at a time: a field of
  # `required` missing, a field the schema does not know, and each field of its
  # own given a value the schema refuses. Three lines, the shape of every check
  # of the table.
  'check	del(.asked) | .zz = 1 | .running = "yes"	.: asked is missing'
  'check	del(.asked) | .zz = 1 | .running = "yes"	.zz: not in the schema'
  'check	del(.asked) | .zz = 1 | .running = "yes"	.running: string, not boolean'
  # The bounds of the schema, of the one field that has one: an issue of 1,
  # the least number the commands take (console/src/stop.rs reads an issue)
  # and 0, which no reading of the command can print.
  'bound	issue	.issue = 1	.issue = 0	under 1'
  'required	["api","issue","running","asked","frozen"]'
  # The listing `tehas history --json` prints: the ends of cycles and of
  # planning turns, oldest first, and the cursor to keep. The two samples are
  # the two readings of the store: three ends on it, and nothing. One `items`
  # schema of the array covers both kinds of line, so a field only one kind
  # prints is optional there and `kind` says which line a field belongs to; the
  # `required` of a line is the whole of what the command prints for either
  # kind (console/src/history.rs). The surface prints no state of its own, so
  # it has no `state` lines and no enum to compare them with.
  'surface	history'
  'sample	sample'
  'sample	empty'
  # The one object of the surface, bent one way at a time: the cursor to keep
  # missing, a field the schema does not know at the top of it, the array of
  # lines given a word, and three ways the plan line of it is wrong - a field
  # every line prints missing, a field the schema does not know, and `kind`, the
  # one field of a line the schema holds to two words, given the word the store
  # calls a cycle rather than the word the command prints. The three lines of a
  # line leave the list as the sample holds it: a word for the array answers the
  # three above it and nothing of a line, and the object of each check is still
  # a listing of the surface.
  'check	del(.next) | .zz = 1 | .cycles = "1"	.: next is missing'
  'check	del(.next) | .zz = 1 | .cycles = "1"	.zz: not in the schema'
  'check	del(.next) | .zz = 1 | .cycles = "1"	.cycles: string, not array'
  'check	del(.next) | del(.cycles[3].outcome)	.cycles[3]: outcome is missing'
  # An unknown field of a line, held by a plan line on purpose: the walk of the
  # bounds below bends one field of `.cycles[3]` and asks the check about the
  # object whole, so a word a line of that sample does not hold would be found
  # wrong there before it is found wrong here.
  'check	del(.next) | .cycles[3].zz = 1	.cycles[3].zz: not in the schema'
  'check	del(.next) | .cycles[3].kind = "cycle"	.cycles[3].kind: "cycle" is not one of ["work","plan"]'
  # The bounds of the schema, each of a field of one line: the least value the
  # schema lets through and one step past it, as the command prints or refuses
  # them. The field of the line names the field as the schema does, with the
  # place of its own line in the listing after it: `cycles[0].issue`. The path
  # the check prints holds both of them, as the drain's `running` is reported at
  # `.running[0]`: a name that goes into an element of a field of a line holds
  # that element too, `cycles[3].filed[0]`, which the check prints as
  # `.cycles[3].filed[0]`. The places are those of the one sample the case of
  # the bounds bends, `api/history.sample.json`, whose first line is the end of a
  # cycle and whose last is the end of a planning turn, so a field of either kind
  # is named on a line that holds it. An issue of 1 and 0, the least
  # number the factory reads and the one it never lists; an exit status of 255,
  # the most this release keeps, and 256; the widest cursor the command prints
  # and one wider still, which it refuses as an argument of --after
  # (console/src/history.rs); the step number and the round of a heartbeat, which
  # the store keeps from 1 to 1000 (console/store/src/live.rs); and the count of
  # a run of parks, which the command gives only from 1 (console/src/history.rs).
  'bound	cycles[0].issue	.cycles[0].issue = 1	.cycles[0].issue = 0	under 1'
  'bound	cycles[0].status	.cycles[0].status = 255	.cycles[0].status = 256	over 255'
  'bound	cycles[0].cursor	.cycles[0].cursor = "9999999999-999999999-999999999999999"	.cycles[0].cursor = "12345678901-12-1"	does not match'
  'bound	cycles[0].stepNumber	.cycles[0].stepNumber = 1000	.cycles[0].stepNumber = 1001	over 1000'
  'bound	cycles[0].repeat	.cycles[0].repeat = 1	.cycles[0].repeat = 0	under 1'
  # The round of the plan line: the store keeps a heartbeat's step number, total
  # and round from 1 to 1000 (console/store/src/live.rs), and a line prints them
  # only when its end copied them from a heartbeat of its own step.
  'bound	cycles[3].round	.cycles[3].round = 1	.cycles[3].round = 0	under 1'
  # The park reason of a line, which the plan line of that sample does not
  # print: the store keeps an empty word as no reason at all
  # (console/store/src/cycle.rs), so the least the schema lets through is one
  # word of a reason and the step past it nothing, which no line prints.
  'bound	cycles[3].parkReason	.cycles[3].parkReason = "merge"	.cycles[3].parkReason = ""	shorter than 1'
  # The issues a turn filed, of the one line of either sample that prints it:
  # none at all is what a plan line that filed nothing leaves out, and an
  # element that is no text is one step past what the command ever puts there
  # (console/src/history.rs keeps the addresses and drops the rest).
  'bound	cycles[3].filed[0]	.cycles[3].filed = []	.cycles[3].filed = [1]	integer, not string'
  'required	["api","cycles","next"]'
)

# The limit of the `limit` samples, as it is set: one cycle at a time, no
# planning, an expiry forty minutes ahead and a reason. `expired` is the same
# file read later, so a `--until` in the past is never given: console flags
# refuses one with 64, at the moment it is set at. The reading of the file, not
# the writing of it, is what a sample is, so `set` prints its words and not
# `--json`: as the last argument of set, `--json` is refused as an option
# without its value. The reason is one line, so the field prints; a limit with
# no reason leaves it out, and `until` is the field the samples of the limit
# surface do not print at all, so the walk of the fields over all four of them
# finds it only in the two that hold an expiry.
limit_set=(--cycles 1 --plan off --until 2026-10-17T15:00:20Z --reason held)
# 2026-10-17T14:00:20Z, the moment the limit above is in force at, and twenty
# minutes on, when it has expired.
now_ok=1792245620
now_expired=1792249280
# The console's words for a reading. Every sample is one, printed the same
# line: the arguments of the limit belong to the set that wrote the file, never
# to the print.
limit_print=(flags limit show --json)

# <name>: the moment that sample is read at. `empty` and `unreadable` print
# nothing beyond `state`, so the moment makes no difference to either. The
# limit is written at the moment it is in force at whatever it is read at, as
# `set` refuses a `--until` of the past: `expired` is that file, read later.
limit_now() { # <name>
  [ "$1" = expired ] && printf '%s\n' "$now_expired" || printf '%s\n' "$now_ok"
}

# The two samples of `drain` differ only by the lock below: an issue's cycle
# that holds it for a minute, as test/drain_test.sh holds one. The drain takes
# no lock of its own, so 0 is a wait long enough for the lock to be tried and
# not one the case waits through, and `running` is the list made at once.
drain_hold=0
# 12, the issue of test/drain_test.sh whose cycle holds its lock for the
# `running` sample: 12.lock is the file drain finds and names.
drain_issue=12

# <name>: the world that sample is a reading of. Both of them are drained by
# the command itself, so the state it finds is one the case puts there: nothing
# for `sample`, and issue 12's cycle lock held for a minute by a sleeper of the
# case for `running`. The sleeper is killed when the case ends, so a later case
# never inherits a lock no one means to hold.
drain_world() { # <name>
  [ "$1" = running ] || return 0
  mkdir -p "$T/home/tehas/state/cycles"
  printf '#!%s\nexec 7>"%s/%s.lock" && flock 7\nsleep 60\n' \
    "$T_BASH" "$T/home/tehas/state/cycles" "$drain_issue" >"$T/bin/held-cycle"
  chmod +x "$T/bin/held-cycle"
  env -i PATH="$T/bin:$PATH" HOME="$T/home" TEHAS_HOME="$T/home/tehas" \
    CYCLE_LOCK="$T/home/tehas/state/cycles/$drain_issue.lock" held-cycle \
    >/dev/null 2>&1 &
  t_after="kill $! 2>/dev/null; wait $! 2>/dev/null"
  while kill -0 "$!" 2>/dev/null && flock -n "$T/home/tehas/state/cycles/$drain_issue.lock" true; do
    sleep 0.02
  done
}

# <name>: the reading a drain sample holds, as the command prints it. The wait
# is named on the command line, and so is --json: the object is the whole of
# standard output, since every word the command says to a person goes to
# standard error.
drain_reading() { # <name>
  t_run "'$T_ROOT/bin/tehas' drain --wait $drain_hold --json"
}

# The seconds of the ends the history samples are read from, as the cursors of
# api/history.sample.json name them: four ends of three issues, the first at
# 2025-10-17T11:20:00Z and the last five minutes on. Named here so the writing
# of the ends and the words of the samples say the same moments.
history_end=1760700000

# <name>: the world that sample is a reading of. `empty` is a store with no end
# on it, which is the state of the scratch home of `printed` before any end is
# written, so nothing is written for it at all. The moments of the ends are the
# ones the cursors of the sample name, and every end goes through a helper of
# test/lib.sh that writes it the way the unit that ends a cycle or a turn does.
history_world() { # <name>
  local home=$T/home/tehas parked started a
  local -a end=()
  # The cycle of #12 holds its lock the whole of its run, so the file is there
  # before the store is asked for a start of it, as t_started makes it itself.
  mkdir -p "$home/state/cycles"
  # The store of the reading goes in at the start of every world, including the
  # one with no ends on it: `empty` is a store with nothing ended, which is not
  # a home with no store at all, and the command prints no object for that one.
  # `printed` takes the home away and writes an instance before each reading, so
  # this is where a history world makes its store, as t_store_home does for the
  # file of a record test.
  t_store_home "$home"
  [ "$1" = empty ] && return 0
  # The park both ends of #12 stopped on: the same words each time, since a run
  # of parks counts only the ends that stopped exactly alike (console/src/history
  # .rs), and the second of them is the line that carries `repeat`. The reason of
  # the park is left out of both: the store keeps an empty word as no reason at
  # all (console/store/src/cycle.rs), so a park that names none prints no
  # `parkReason`. The end of #9 below is the line that does name one.
  parked='{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"","failure_kind":"verify-failed","failure_cause":"exit-1","parked":true}'
  # The first end, written the steps t_history_end takes for an end with `by`,
  # and with a heartbeat between its start and its end: a heartbeat is refused
  # before the cycle starts and after it ends (console/store/src/live.rs), and
  # the end copies the step number, the total and the round only from a
  # heartbeat of its own step, written at the end's own time
  # (console/store/src/cycle.rs). The helper gives no step number, so the three
  # fields of the sample are this end's and no other's. The pull request of the
  # cycle is its number: the command gives the address of the repository the
  # instance names, so the sample is one of acme/widget.
  started=$((history_end - 60))
  a=$(TEHAS_HOME=$home "$T_STORE" cycle-show 12 | jq '.cycle.attempt // 0')
  t_approve "$home" 12 alice label $((started - 1 - a))
  TEHAS_STORE_TEST_NOW=$started t_started "$home" 12
  TEHAS_HOME=$home TEHAS_STORE_TEST_NOW=$((started + 30)) "$T_STORE" \
    cycle-heartbeat 12 --running --status running --step develop \
    --step-number 7 --step-total 9 --round 3 >/dev/null ||
    t_bad "the store refused the heartbeat of #12"
  mapfile -t end < <(jq -r '
    "--status", .status,
    "--rc", (.rc | tostring),
    "--step", .step,
    "--cap", .cap,
    "--failure-kind", .failure_kind,
    "--failure-cause", .failure_cause,
    "--pr", "9",
    (if .parked == true then "--parked" else empty end)' <<<"$parked")
  TEHAS_HOME=$home TEHAS_STORE_TEST_NOW=$history_end \
    flock -w 5 "$home/state/cycles/12.lock" "$T_STORE" cycle-end 12 \
    --attempt $((a + 1)) "${end[@]}" >/dev/null ||
    t_bad "the store refused the first end of #12"
  # The second park of #12, stopped exactly as the first, so it is the line that
  # carries `repeat`. No by: a by-hand start names no account, and `by` is left
  # out of the line, which is one of the fields the samples hold nowhere else.
  t_history_end "$home" 12 "$((history_end + 60))" "$parked"
  # The end of another issue, stopped as those two did but on a park that names
  # its reason, so this is the line that prints `parkReason`. An issue of its
  # own, because a run of parks counts none of another (console/src/history.rs):
  # the line says nothing of a repeat, and it names no pull request either, so
  # `pr` is the field of the first line alone.
  t_history_end "$home" 9 "$((history_end + 120))" \
    '{"kind":"work","rc":10,"status":"failed","step":"develop","cap":"step-failed:develop","park_reason":"findings-open","failure_kind":"review","failure_cause":"findings-open","provider":false,"parked":true}'
  # The plan end: a refine the factory started on its own, whose turn filed an
  # issue, so `label`, `auto` and `filed` are the fields of a line of that kind.
  # Its turn began with the minute the park of #12 ended, so the two lines of
  # the listing name a turn that ran while the cycle of the issue before it ran.
  t_plan_end "$home" 7 "$((history_end + 300))" \
    '{"kind":"plan","started":1760700000,"rc":0,"status":"finished","turn":1,"by":"alice","label":"tehas:refine","filed":["https://github.com/acme/widget/issues/41"],"auto":true}'
}

# <name>: the reading a history sample holds, as the command prints it. The
# object is the one jq prints of the reading, as every sample of the table is
# written.
history_reading() { # <name>
  t_run "'$T_ROOT/bin/tehas' history --json | jq ."
}

# The moment the limit file is written at: 2026-10-17T14:00:20Z, when it is in
# force, whichever reading of it a sample is. `set` takes the same moment for
# its own now, so it never sees a `--until` of the past. Named here so the two
# places that give it to a command say the same thing.
limit_written=1792245620

# <name>: the state of the world that sample is a reading of: the limit file,
# or a directory where it would be. `sample` and `expired` are one file, the one
# the set below writes, read at two moments: an expired limit is a limit that was
# set while it was still in force, so the write happens at the moment of the
# reading that is in force and not at the moment of its own. Each run of the
# console goes through t_run, which builds its environment from nothing: HOME and
# TEHAS_HOME of the run's own, so the file lands under the scratch tree and
# nowhere else. The clock of the run that writes the file goes in the snippet
# itself, since an assignment before the call would not reach a command t_run
# builds with env -i. The words `set` prints go to standard error, where they are
# left out of `t_out` entirely, so the whole of `t_out` after such a run is the
# object of a reading and nothing else: the object a caller takes from `printed`
# is the one reading and not those words above it.
limit_world() { # <name>
  case $1 in
    sample | expired) t_run "TEHAS_FLAGS_TEST_NOW=$limit_written '$T_CONSOLE' \
        flags limit set ${limit_set[*]} 2>&1 >/dev/null" ||
      t_bad "setting the limit failed: $t_out" ;;
    empty) ;;
    unreadable) t_run "mkdir -p '$T/home/tehas/state/limit.json'" ;;
  esac
}

# <name>: the reading a sample holds, as the console prints it. The moment it
# is read at goes in the snippet for the same reason the clock of the write
# does; the object is the one jq prints, as every sample in api/ is written.
limit_reading() { # <name>
  t_run "TEHAS_FLAGS_TEST_NOW=$(limit_now "$1") '$T_CONSOLE' ${limit_print[*]} | jq ."
}

# Where the three files of issue 7's cycle are, and the holder that stands for
# the cycle itself: a cycle takes its lock for the whole of its run and ends with
# it, so the holder must be alive while a reading is made and gone when the case
# is done. It is a child of this shell, and a run of t_run waits for the children
# of the shell it runs in, so the holder is made of its own session: adopted, not
# waited for (test/lib.sh holds a lock the same way in t_hold, and test/drain_test
# .sh holds a cycle's lock as a poll's child for the same reason).
freeze_home=$T/home/tehas/state/cycles
freeze_holder=

# The holder is let go, once only, so a reading that made none is never asked to
# free a lock another reading took.
freeze_holder_end() {
  [ -n "$freeze_holder" ] || return 0
  kill "$freeze_holder" 2>/dev/null
  freeze_holder=
}

# The holder, of its own session so no run of t_run waits for it. The body is in
# single quotes on purpose: the path is the argument at the place of `$1`.
freeze_holder_start() { # <file>
  freeze_holder_end
  # shellcheck disable=SC2016  # $1 is the file, given to the snippet as its word
  setsid bash -c 'exec 9>"$1" && flock 9 && exec sleep 600' _ "$1" &
  freeze_holder=$!
  while kill -0 "$freeze_holder" 2>/dev/null &&
    flock -n "$1" true; do command -p sleep 0.02; done
}

# The three files of issue 7's cycle that say whether a freeze is asked and
# whether the watch has paused the container, as the watch leaves them: the lock
# held by the holder, the request `tehas freeze` writes, and the pause the watch
# writes. A lock no process holds is read as no cycle, and one a cycle leaves
# behind when it ends says nothing on its own either (console/src/freeze.rs, and
# the one probe of console/core/src/lock.rs that `tehas drain` reads the same
# way). So `idle` is no lock, no request and no pause. `asked` is the lock held,
# as test/drain_test.sh holds a running cycle's lock, with no request yet, and is
# read by the freeze that asks for one. `frozen` adds the two files the watch
# leaves, and is read by `tehas freeze` again, which answers it is frozen already.
# `thaw` is that same world read by `tehas thaw`, which removes the request and
# leaves the pause for the watch to take away.
freeze_world() { # <name>
  mkdir -p "$freeze_home"
  case $1 in
    idle) ;;
    asked) freeze_holder_start "$freeze_home/7.lock" ;;
    frozen | thaw)
      freeze_holder_start "$freeze_home/7.lock"
      printf '{}\n' >"$freeze_home/7.freeze"
      printf '1600\n' >"$freeze_home/7.frozen"
      ;;
  esac
}

# The reading of one freeze sample: the command, with `--json`, in the world the
# case made. The verb is the other half of the surface's name: `tehas freeze`
# prints `idle`, `asked` and `frozen`, and `tehas thaw` prints `thaw`, the answer
# of the same object after the request was removed. The object is the one jq
# prints, as every sample of the table is written.
freeze_reading() { # <name>
  local verb=freeze
  [ "$1" = thaw ] && verb=thaw
  t_run "'$T_CONSOLE' tehas $verb 7 --json | jq ."
}

# The holder of one reading's cycle is ended with the reading, and again as every
# case ends, so a reading that could not be made leaves no lock for the rest of
# the file to read as a cycle that never ends.
t_after='freeze_holder_end'

# <surface> <name>: the object the code prints for that sample, as committed, on
# standard output as one JSON object. The console of this run prints it, and the
# same program wrote the file. Not bin/tehas: that goes through console front,
# whose load of the instance drops every TEHAS_ name the settings hold no value
# for, the test clock among them, so nothing can be read at a moment of its own
# through that door. The object is the one jq prints of the reading, so a caller
# may put it in a file or pipe it to jq and get one object back, and not the
# words of the write that made the file.
printed() { # <surface> <name>
  local surface=$1 name=$2
  rm -rf "$T/home/tehas"
  t_instance "${T_MINIMAL[@]}"
  case $surface in
    limit)
      limit_world "$name"
      limit_reading "$name"
      printf '%s\n' "$t_out" | jq .
      ;;
    drain)
      drain_world "$name"
      drain_reading "$name"
      printf '%s\n' "$t_out" | jq .
      ;;
    history)
      history_world "$name"
      history_reading "$name"
      printf '%s\n' "$t_out"
      ;;
    freeze)
      freeze_world "$name"
      freeze_reading "$name"
      # The reading reads the three files and takes no lock of its own, so the
      # answer is the world as the case wrote it; the holder goes with the
      # reading, so nothing later waits for a cycle that has ended.
      freeze_holder_end
      printf '%s\n' "$t_out"
      ;;
    *) return 1 ;;
  esac
}

# The `required` of the surface, as the table's `required` line gives it.
required_json=

# The object a surface's `sample` is, read once from api/ and held here for the
# checks that bend one field of it and ask the schema about the result. Named
# `bound_object` and not `object`, which the loop of the table already uses for
# one word of a `check` line.
bound_object=

# The reading of one sample, made here and not read from api/: the world of the
# sample is built and the console runs, so the object of that reading is printed
# on standard output as one JSON object. A caller that means to prove a file of
# api/ against what the code prints now takes it from a command substitution;
# one that means to ask what the code prints for a field pipes it to jq. Either
# way the object comes from this function alone, so nothing else of a run, the
# words of the write among them, can come out with it.
sample_object() { # <surface> <name>
  printed "$1" "$2"
}

# <path>: the state of one sample, as JSON text, from the reading the console
# makes and not from the file of api/: a sample that stopped printing its state
# is found here and not agreed with. Nothing when the reading holds no state.
sample_field_state() { # <surface> <name> <path>
  local object
  object=$(sample_object "$1" "$2") || return 1
  [ -n "$object" ] || return 1
  printf '%s\n' "$object" |
    jq -c --arg path "$3" 'getpath($path | split(".")) | if . == null then empty else . end'
}

# A sample of the surface that the `check` and `bound` lines of the table bend
# one field of: `api/<surface>.sample.json` where the surface has one, and the
# first of the surface's samples where it has none. The two surfaces of this
# table read their object differently and the same one line, so a surface that
# takes the sample of its answers another name is one line here and not a
# second kind of row. The object is read from api/, which is what the two cases
# that use it are asked about: the check of a bent sample.
surface_sample() { # <surface>
  local file=$api/$1.sample.json
  [ -f "$file" ] || file=$api/$1.${names[0]}.json
  printf '%s\n' "$file"
}

check() { # <schema file> <json file>: the lines test/schema.jq prints for it
  local out rc
  out=$(jq -r --slurpfile schema "$1" -f "$T_ROOT/test/schema.jq" "$2" 2>&1)
  rc=$?
  # A jq that could not read its inputs prints nothing at all and would answer
  # the caller's comparison against the empty string with a pass, so its status
  # is what this says: 0, and the lines it printed.
  if [ "$rc" != 0 ]; then
    printf 'the check of %s against %s exited %s: %s\n' "$2" "$1" "$rc" "$out" >&2
    return 1
  fi
  printf '%s\n' "$out"
}

# Every field the schema names, as a path: cycles, until.
schema_paths() { # <schema file>
  local out rc
  out=$(jq -r 'def walk_props($p): (.properties // {}) | to_entries[]
             | ($p + .key) as $q | $q, (.value | (.items // .) | walk_props($q + "."));
             walk_props("")' "$1" 2>&1)
  rc=$?
  if [ "$rc" != 0 ]; then
    printf 'the fields of %s could not be listed (exit %s): %s\n' "$1" "$rc" "$out" >&2
    return 1
  fi
  printf '%s\n' "$out" | sort -u
}

# The paths of a value, as the same dotted names. jq's `paths` holds the one
# element of an array as a path of its own, so an array of objects gives its
# fields once, as a schema names them. A file jq cannot read is named on
# standard output, since the caller reads what comes back from a pipeline.
held_paths() { # <json file>
  local out rc
  out=$(jq -r 'paths | map(strings) | join(".") | select(. != "")' "$1" 2>&1)
  rc=$?
  if [ "$rc" != 0 ]; then
    printf '%s could not be read (exit %s): %s\n' "$1" "$rc" "$out"
    return 1
  fi
  printf '%s\n' "$out" | sort -u
}

# The enum a schema holds at one path of its fields, as JSON, and nothing when
# it holds none there. Each field of the path is taken under `properties`, so
# the state a surface prints itself is held by its path alone, `state`, and the
# state of the limit a drain carries by `limit.state`, which lies under the
# properties of the object that carries it.
enum_of() { # <schema file> <path>
  jq -c --arg path "$2" 'def at($p): reduce ($p | split("."))[] as $f
           (.; getpath(["properties", $f])); at($path).enum' "$1"
}

# The path the check prints for the field of a `bound` line of a listing. The
# name of such a line holds the field as the schema names it and the place of
# its own line in the array of them, `cycles[0].issue`; a name that stops at a
# field of a line that holds a list leaves that field's element out,
# `cycles[3].filed`, the way the drain's `running` is named without its element.
# The check reports the whole path, `.cycles[0].issue` and
# `.cycles[3].filed[0]`, so the one element a name leaves out is put back here.
path_of_bound() { # <field>: the dotted path the check prints for it
  local field=$1 element=']' '[' '0' ']'
  printf '%s\n' "${field/$element/${element[0]}${element[1]}${element[2]}${element[3]}}"
}

# A line of the table is read as its kind and the words after it, one apart on
# a tab: the kind of the first, so a `check` line keeps every space of its jq
# and of the line it is reported with, and neither of the two holds the tab
# that divides them. The word at that place is printed; 1 says the line is of
# another kind or holds no word there, which is what ends a surface. A function
# never ends on `&&`, which would answer 0 for a word it did not print and
# leave the caller with nothing to name a sample by.
surface_line() { # <line> <field>: the tab-divided word at that place
  local line=$1 field=$2 n=0 out=() w IFS=$'\t'
  read -r -a out <<<"$line"
  for w in "${out[@]}"; do
    n=$((n + 1))
    if [ "$n" = "$field" ]; then
      printf '%s\n' "$w"
      return 0
    fi
  done
  return 1
}

i=0
while [ "$i" -lt "${#surfaces[@]}" ]; do
  line=${surfaces[i]}
  if [ "$(surface_line "$line" 1)" != surface ]; then
    t_case "the table's line $((i + 1))"
    t_bad "neither a surface, a sample nor a check: $line"
    i=$((i + 1))
    continue
  fi
  surface=$(surface_line "$line" 2)
  [ -n "$surface" ] || {
    t_case "the table's line $((i + 1))"
    t_bad "a surface with no name: $line"
    i=$((i + 1))
    continue
  }
  i=$((i + 1))
  names=()
  states=()
  # The path the state of a sample is read at, and the surface that owns the
  # enum of it: the two words of the `state_path` line after its kind, and
  # nothing at all for a surface that prints its own state, which lists the enum
  # with its `state` lines.
  state_path=state
  state_owner=
  cases=()
  bounds=()
  required_json=
  while [ "$i" -lt "${#surfaces[@]}" ]; do
    line=${surfaces[i]}
    kind=$(surface_line "$line" 1)
    case $kind in
      sample) names+=("$(surface_line "$line" 2)") ;;
      state_path)
        # The path and the surface that owns the enum held there, both of them
        # named or the line is no `state_path` line: a surface that carries the
        # object of another says whose it is, so its states are asked of that
        # surface's samples and its enum is asked to be that one's.
        state_path=$(surface_line "$line" 2)
        state_owner=$(surface_line "$line" 3)
        ;;
      state) states+=("$(surface_line "$line" 2)") ;;
      required) required_json=$(surface_line "$line" 2) ;;
      check)
        object=$(surface_line "$line" 2)
        cases+=("$object"$'\t'"$(surface_line "$line" 3)")
        ;;
      bound)
        bounds+=("$(surface_line "$line" 2)"$'\t'"$(surface_line "$line" 3)"$'\t'"$(surface_line "$line" 4)"$'\t'"$(surface_line "$line" 5)")
        ;;
      *) break ;;
    esac
    i=$((i + 1))
  done
  schema=$api/$surface.schema.json

  t_case "api/$surface.schema.json is 2020-12, closed, and names its test"
  # The required list is the table's, so nothing of one surface is written in
  # the loop that is meant to hold every surface.
  if [ -z "$required_json" ]; then
    t_bad "the table names no required list for $surface"
  elif [ "$(jq -c '.required' "$schema")" != "$required_json" ]; then
    # Named as both sides, so the case that did not check says what it found.
    t_bad "the required of the schema is $(jq -c '.required' "$schema" 2>&1), the table says $required_json"
  fi
  t_eq "$(jq -r '."$schema"' "$schema")" "https://json-schema.org/draft/2020-12/schema"
  t_eq "$(jq -r '."$id"' "$schema")" "https://github.com/tehas-app/tehas/api/$surface.schema.json"
  t_eq "$(jq '.additionalProperties' "$schema")" "false"
  # The description is kept to what a keyword of this file or a case below
  # carries: version 1 (`api` is the const 1), that objects are closed
  # (additionalProperties is false below), where each field is documented, and
  # which test checks the samples. The case after the keywords proves only that
  # the name of the test appears, so it is not a check of what the description
  # claims: a claim about how the command behaves at runtime belongs in
  # host/docs/api.md, which the manual case below does hold to the fields of the
  # schema. A claim added here that nothing checks is a fault of review, not one
  # this file can catch.
  # Every keyword of the schema is one the check looks at, so none is passed
  # over in silence: the keys of every object that holds fields of its own are
  # a keyword of the list test/schema.jq knows.
  t_eq "$(jq -c '[.. | objects | select(has("properties") or has("const") or
      has("enum") or has("type")) | keys_unsorted[] | select(IN("$schema", "$id",
        "title", "description", "type", "const", "enum", "required",
        "additionalProperties", "properties", "items", "minimum", "maximum",
        "minLength", "maxLength", "pattern", "maxItems", "format") | not)]' "$schema")" "[]"
  t_contains "$(jq -r '.description' "$schema")" "test/json_contract_test.sh"

  # The state of every sample, taken from what the console prints and not from
  # the file in api/, and the whole list compared to the enum: a state of the
  # enum that no sample prints is refused, and so is a sample that prints
  # another state than the row of the table names for it. A surface whose schema
  # names no state of its own has no enum and no `state` lines, and nothing of
  # its samples is a state of anything: the other cases below still hold its
  # samples to the schema, to what the command prints and to the walk of its
  # fields.
  t_case "every state of the schema of $surface is printed by one of its samples"
  if [ -z "$state_owner" ] && [ "$(jq -r '.properties | has("state")' "$schema")" != "true" ]; then
    [ "${#states[@]}" = 0 ] ||
      t_bad "the table names ${#states[@]} states for a schema of $surface with no state"
  else
    [ "${#names[@]}" = "${#states[@]}" ] ||
      t_bad "the table names ${#states[@]} states for ${#names[@]} samples of $surface"
    for n in "${!names[@]}"; do
      # The object of the reading comes from sample_object, which runs the console
      # and prints the object of the reading and nothing else; jq here picks the one
      # field out of it. Read from api/ the state would only be agreed with.
      printed_state=$(sample_field_state "$surface" "${names[$n]}" "$state_path") ||
        t_bad "the console printed no object for $surface ${names[$n]}"
      [ -n "$printed_state" ] ||
        t_bad "the reading of $surface ${names[$n]} holds no state at $state_path"
      t_eq "$printed_state" "\"${states[$n]}\""
    done
    # The enum the samples are read against, held by the schema at the path the
    # table names. For a surface that prints its own state the table's `state` lines
    # are the whole of it, so a state of the enum no sample prints is refused here
    # and so is a state the schema does not name. For a surface that only carries the
    # object of another the table names the enum and the surface that owns it: the
    # enum of this schema is asked to be the enum of that one, and every state these
    # samples print is asked to be one of them.
    if [ -n "$state_owner" ]; then
      t_eq "$(enum_of "$api/$state_owner.schema.json" state)" \
        "$(enum_of "$schema" "$state_path")"
      for state_word in "${states[@]}"; do
        case $(enum_of "$schema" "$state_path") in
          *"\"$state_word\""*) ;; *) t_bad "$state_word is no state of the schema of $surface" ;;
        esac
      done
    else
      t_eq "$(printf '%s\n' "${states[@]}" | jq -R . | jq -sc 'sort')" \
        "$(enum_of "$schema" "$state_path" | jq -c 'sort')"
    fi
  fi

  if [ -n "$samples_write" ]; then
    for name in "${names[@]}"; do
      printed "$surface" "$name" >"$api/$surface.$name.json" ||
        t_bad "the rewrite printed no object for $surface $name"
    done
  fi

  for name in "${names[@]}"; do
    t_case "api/$surface.$name.json is what the code prints now"
    # Both sides are read here rather than one of them handed to diff as a
    # process substitution, and each is refused when it is no object: a side
    # that failed prints nothing, and two sides that print nothing do not
    # differ. The object of the file is the one jq prints of it, which is how
    # every sample in api/ is written.
    printed_now=$(printed "$surface" "$name") ||
      t_bad "the console printed no object for $surface $name"
    [ -n "$printed_now" ] ||
      t_bad "the console printed nothing for $surface $name"
    committed=$(jq . "$api/$surface.$name.json" 2>&1) ||
      t_bad "$api/$surface.$name.json holds no object: $committed"
    if ! changes=$(diff <(printf '%s\n' "$printed_now") <(printf '%s\n' "$committed")); then
      t_bad "the code prints another object; when the change is only an addition, write the samples again:
$changes"
    fi

    t_case "api/$surface.$name.json fits api/$surface.schema.json"
    lines=$(check "$schema" "$api/$surface.$name.json") ||
      t_bad "the schema check could not be run"
    t_eq "$lines" ""
  done

  t_case "the rewrite prints what api/$surface holds"
  # The rewrite of TEHAS_SAMPLES_WRITE is run here into its own directory and
  # every file of it is compared to the one in api/, so the mode a contributor
  # reaches for after a change to the console is a case of this file and not
  # only a line of the header: a rewrite that lost a clock, a world or the
  # shape jq prints fails here instead of shipping green. The files of api/ are
  # read and never written by this case.
  rewrite=$T/rewrite
  mkdir -p "$rewrite"
  for name in "${names[@]}"; do
    sample_object "$surface" "$name" >"$rewrite/$surface.$name.json" ||
      t_bad "the rewrite printed no object for $surface $name"
  done
  for name in "${names[@]}"; do
    t_eq "$(cat "$rewrite/$surface.$name.json")" "$(cat "$api/$surface.$name.json")"
  done

  # Every field of the `required` of the schema, as one path per field, held by
  # every sample. The walk below of the fields asks only that some sample holds
  # a field, which is right for a field a reading may leave out; a field of
  # `required` may not be left out of any reading, so each sample is asked on its
  # own, and a schema that names a field none of them prints is refused here and
  # not by a walk over all four together.
  t_case "every sample of $surface holds every field its schema requires"
  missing=""
  for name in "${names[@]}"; do
    for path in $(jq -r '.required[]' "$schema"); do
      [ -n "$path" ] || { t_bad "the required of $surface names an empty field"; continue; }
      case $(held_paths "$api/$surface.$name.json") in
        *"$path"*) ;; *) missing+=" $surface.$name:$path" ;;
      esac
    done
  done
  t_eq "$missing" ""

  t_case "the samples of $surface between them hold every field of its schema"
  paths=$(schema_paths "$schema") || t_bad "the schema could not be walked"
  # A schema whose fields cannot be named would compare equal to a walk of
  # nothing, so both sides of the comparison are said here when either is empty.
  [ -n "$paths" ] || t_bad "the schema of $surface names no field"
  held=$(for name in "${names[@]}"; do held_paths "$api/$surface.$name.json"; done | sort -u)
  case $held in *"could not be read"*) t_bad "a sample of $surface holds no object:$held" ;; esac
  # The fields under the limit a surface carries are carried by the samples of
  # the surface that prints that object itself, so they are asked of it and not of
  # these: for a drain that is every field of the limit but `until`, which no
  # drain can hold because its limit is in force to no time and lasts until it is
  # cleared (host/docs/api.md). A drain that ever printed an `until` is refused by
  # the case above, which asks both samples for the object the command prints now.
  carried=
  [ -z "$state_owner" ] ||
    carried=$(jq -r 'def walk_props($p): (.properties // {}) | to_entries[]
             | ($p + .key) as $q | $q, (.value | (.items // .) | walk_props($q + "."));
             walk_props("limit.")' "$schema" | sort -u)
  wanted=$(comm -23 <(printf '%s\n' "$paths") <(printf '%s\n' "$carried"))
  wanted=$(printf '%s\n' "$wanted" | grep -vx 'limit\.until')
  [ -n "$held" ] || t_bad "no sample of $surface holds a field"
  t_eq "$(comm -23 <(printf '%s\n' "$wanted") <(printf '%s\n' "$held"))" ""

  t_case "the manual names every field of the schema of $surface"
  manual=$(cat "$T_ROOT/host/docs/api.md")
  unnamed=""
  for path in $paths; do
    case $manual in *"| \`$path\` |"*) ;; *) unnamed+=" $path" ;; esac
  done
  t_eq "$unnamed" ""

  # The freeze page owes one more thing to its reader than the fields: the three
  # files of the cycle are the whole of what the two commands read, so the page
  # names them under the fields it gives, as it does for every other surface of
  # this table. Named by the sentence the page carries, so the case fails when the
  # page drops it.
  if [ "$surface" = freeze ]; then
    t_case "the manual says where the fields of $surface are read from"
    t_contains "$manual" "The three files of the cycle named under"
  fi

  t_case "the check finds a field of $surface missing, one of the wrong type, and one it does not know"
  sample=$(surface_sample "$surface")
  for case_line in "${cases[@]}"; do
    if ! jq "${case_line%%$'\t'*}" "$sample" >"$T/bad.json"; then
      t_bad "the jq of the table's line is no jq: ${case_line%%$'\t'*}"
      continue
    fi
    lines=$(check "$schema" "$T/bad.json") || t_bad "the schema check could not be run"
    t_contains "$lines" "${case_line#*$'\t'}"
  done

  # A bound of the table is the field, the jq that gives it the least value the
  # schema lets through, the jq that gives it one step past that, and the word the
  # check must answer with, the four words of the line as `surface_line` divides
  # them. Each jq sets that one field of the sample and leaves every other field
  # of it alone, so the object the check is asked about is still an object of the
  # surface. Each value is one the command really prints or really refuses, so a
  # bound the schema states is a bound the code has a reading on both sides of:
  # `cycles` of 0 and one below it, an `until` with the second 60, which the
  # reader of console/core/src/limit.rs takes, and one with 61, which it reads as
  # unreadable, and a reason of 200 characters, the most it keeps, and one of 201.
  # The keyword case above proves only that a keyword is one the check knows; this
  # proves a bound the schema states is kept, so a bound no reading of the command
  # could ever break has a case that fails without it. A ceiling the command
  # refuses when its arguments are read is not stated here as a bound of the
  # field: host/docs/api.md gives it with the rule and the file that enforces it.
  # The limit of a surface that carries one is the limit of the surface that
  # owns it, with the `api` of its properties taken out, as api/ has no $ref for
  # the one to point at the other (see api/disk-cycles.schema.json, which
  # restates what tehas status prints the same way). The whole of the object is
  # compared to the whole of it: its type, its closedness, the fields it may hold
  # and their bounds, so a keyword, a bound or a state of the enum that one file
  # has and the other has not is named here and not left for a reader to notice.
  # The `required` of the two are not the same and no field is taken from either,
  # so the one of the owner is named as what the copy does not carry: an object of
  # this surface holds a state and no api, since the one it prints sits above it,
  # at the top level (console/src/flags.rs, drain_report).
  if [ -n "$state_owner" ]; then
    t_case "the limit of the schema of $surface is the schema of $state_owner, less api"
    t_eq "$(jq -cS '.properties.limit.properties | del(.api)' "$schema")" \
      "$(jq -cS '.properties | del(.api)' "$api/$state_owner.schema.json")"
    t_eq "$(jq -c '.properties.limit.required' "$schema")" '["state"]'
  fi

  # A bound of a path under another object is reported at the path the check
  # prints, which holds the element of an array and so is not the table's own
  # path: `.running` is reported as `.running[0]`, and a field of a line of a
  # listing as `.cycles[0].filed[0]`.
  t_case "the check keeps the bounds of the schema of $surface"
  bound_object=$(jq -c . "$(surface_sample "$surface")") ||
    t_bad "$(surface_sample "$surface") holds no object"
  for bound in "${bounds[@]}"; do
    bound_field=$(surface_line "$bound" 1)
    ok=$(surface_line "$bound" 2)
    bad=$(surface_line "$bound" 3)
    word=$(surface_line "$bound" 4)
    printf '%s\n' "$bound_object" >"$T/before.json"
    jq ". | $ok" "$T/before.json" >"$T/bound.json" ||
      { t_bad "the jq at the bound of $bound_field is no jq: $ok"; continue; }
    lines=$(check "$schema" "$T/bound.json") || t_bad "the schema check could not be run"
    t_eq "$lines" ""
    printf '%s\n' "$bound_object" >"$T/before.json"
    jq ". | $bad" "$T/before.json" >"$T/bound.json" ||
      { t_bad "the jq past the bound of $bound_field is no jq: $bad"; continue; }
    lines=$(check "$schema" "$T/bound.json") || t_bad "the schema check could not be run"
    case $bound_field in
      # The path the check prints for a bound holds the element of every array it
      # goes through, which no name of the table ever holds: `.running[0]` for the
      # drain's `running`. The names of a listing hold the place of their own
      # line, so the element of the array of lines is put back where the check
      # puts it, and the element of a field of a line that holds a list is put
      # back too: `cycles[0].issue` is reported at `.cycles[0].issue` and
      # `cycles[3].filed` at `.cycles[3].filed[0]`.
      running) t_contains "$lines" ".running[0]:" ;;
      cycles.*) t_contains "$lines" ".$(path_of_bound "$bound_field"):" ;;
      *) t_contains "$lines" ".$bound_field:" ;;
    esac
    t_contains "$lines" "$word"
  done
done

t_done
