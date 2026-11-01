# shellcheck shell=bash
# What of the follow-up steps is read before the console can be asked: the waits
# for the update lock and the dispatcher's, which tehas update and install.sh read
# before tehas_load and before any build, and the record of steps owed, which
# install.sh --check reads on a checkout with no console built. The steps are in
# console/src/update/steps.rs.
#
#   tehas_read_steps          the lines of the record of steps owed, one per line;
#                             1 when there is a record this cannot read
#   tehas_pending_file        where the steps a run asked for and did not finish
#                             are kept until each one has run
#   tehas_update_lock_wait    the seconds tehas update and install.sh wait for
#                             state/update.lock, read from the environment here
#                             because tehas_load drops what it does not know
#   tehas_dispatch_lock_wait  the seconds install.sh waits for state/dispatch.lock
#
# An updater from before keeps every other function of this file as it loaded
# them, when it reads this one again after install.sh: nothing here unsets one.

# Where the unfinished steps of a run are kept. install.sh's report of what
# changed is one-shot: it is spent by the run that made the copies, so without
# this file a run that stopped partway would leave the host half updated and the
# next run would find nothing to do. Printing a path makes no change.
tehas_pending_file() { printf '%s\n' "${TEHAS_HOME:-$HOME/tehas}/state/update-pending"; }

# The two defaults of the update lock's wait, in the one place both commands read
# them: the default, and the ceiling past which a wait is a mistyped number rather
# than a wait. A refusal must stay instant for the tests that time it, so the
# default is small and the ceiling only keeps a mistake from hanging an operator.
tehas_update_lock_wait_default=10
tehas_update_lock_wait_max=300

# The seconds one of the two commands waits for state/update.lock before it says
# another update is running: TEHAS_UPDATE_LOCK_WAIT, or 10. Called before
# tehas_load, which unsets every TEHAS_* it does not know and does not name this
# one - the knob is not a setting of an instance. The name is read here and
# nowhere else, so the two commands cannot drift apart.
#
# 0 is the refuse-at-once value, so a leading zero has to be asked for a digit
# after it: lib/config.sh's _tehas_is_count drops 0 outright because none of its
# own waits may be zero, which would be wrong here. The length is read before the
# value, as lib/config.sh does for its waits, so a number no comparison can hold
# is refused by its length instead of by a failing comparison.
tehas_update_lock_wait() {
  local v=${TEHAS_UPDATE_LOCK_WAIT:-}
  case $v in
  '' | *[!0-9]* | 0[0-9]*) v=$tehas_update_lock_wait_default ;;
  esac
  { [ "${#v}" -le 3 ] && [ "$v" -le "$tehas_update_lock_wait_max" ]; } ||
    v=$tehas_update_lock_wait_default
  printf '%s\n' "$v"
}

# The seconds install.sh waits for state/dispatch.lock, which a poll holds for
# its whole run: TEHAS_DISPATCH_LOCK_WAIT, or 900. A poll's own bounds (the
# fetch, the low gate, a wait for each start, the GitHub calls) add up to
# minutes, so the update lock's 10 would refuse an install whenever the timer's
# poll is running. 0 refuses at once; more than 3600 or anything else is the
# default. Read from the environment, like the update lock's wait.
tehas_dispatch_lock_wait_default=900
tehas_dispatch_lock_wait() {
  local v=${TEHAS_DISPATCH_LOCK_WAIT:-}
  case $v in
  '' | *[!0-9]* | 0[0-9]*) v=$tehas_dispatch_lock_wait_default ;;
  esac
  { [ "${#v}" -le 4 ] && [ "$v" -le 3600 ]; } || v=$tehas_dispatch_lock_wait_default
  printf '%s\n' "$v"
}

# The lines of that record, without blank ones. No record is an empty answer,
# not a failure. A record that exists but cannot be read is the other case: it
# holds steps somebody owed, and answering "nothing is owed" would let the next
# write replace it with what this run happened to know. Losing a step that was
# owed is the one thing this must not do, so a failed read is a failure.
tehas_read_steps() {
  local file line
  file=$(tehas_pending_file)
  if [ -e "$file" ] && ! [ -r "$file" ]; then
    echo "tehas: cannot read the follow-up record $file" >&2
    return 1
  fi
  if [ -s "$file" ]; then
    # A line is read even when something edited the file without a closing
    # newline: losing a step that was owed is the one thing this must not do.
    while IFS= read -r line || [ -n "$line" ]; do
      [ -n "$line" ] || continue
      printf '%s\n' "$line"
    done <"$file" || {
      echo "tehas: could not read the follow-up record $file" >&2
      return 1
    }
  fi
  return 0
}
