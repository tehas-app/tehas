# shellcheck shell=bash
# The follow-up steps an update finishes after install.sh has copied files.
#
# One place names them, because two commands read the list. install.sh prints
# the steps its changed sources call for, one per line, as "next: <step line>",
# and records the same lines in the file of steps owed before it copies a file.
# A step line is checked here before anything runs it, so what a step may be
# asked for - including the arguments it needs - is fixed by this file, not by
# whoever wrote the line.
# tehas update reads those lines and runs each step as its own command with the
# arguments written on that line. Neither side rewrites a step into a command
# line of its own, so editing what install.sh asks for cannot silently turn a
# performed step into an unperformed one: the id has to be one this file knows.
#
#   tehas_step_ids            every step id this file defines, one per line
#   tehas_steps_for <src>...  the steps these changed sources call for, in the
#                             order they run, one per line
#   tehas_steps_into <src>... the same, into the array tehas_steps
#   tehas_sort_steps <line>.. the same step lines in the order they run
#   tehas_step_id <token>       the id this file defines for that token: the
#                               Protolab's old name reads as the edge step
#   tehas_valid_step <line>   0 when a step line names an id this file knows and
#                             carries the arguments that id requires, and no
#                             others; 1 otherwise
#   tehas_read_steps          the lines of the record of steps owed, one per line;
#                             1 when there is a record this cannot read
#   tehas_write_steps <line>.. replace that record with these lines (it writes)
#   tehas_record_steps <line>.. add these steps to that record (it writes)
#   tehas_pending_file        where the steps a run asked for and did not finish
#                             are kept until each one has run
#   tehas_update_lock_wait    the seconds tehas update and install.sh wait for
#                             state/update.lock, read from the environment here
#                             because tehas_load drops what it does not know
#
# Steps, and what they mean to the command that runs them:
#   units <unit>...   systemd reloads its unit files, then restarts these units,
#                     and only where one is already active
#   sandbox           rebuild the sandbox image
#   docs <flag>       renew the docs container, and only where it is running; the
#                     one flag accepted is --force-recreate and it is required,
#                     because Caddy reads its config when the process starts and
#                     a changed Caddyfile never reaches a container left as it
#                     was. A bare "docs" is not this step: it would run the
#                     weaker action and still be marked done.
#   edge <flag>       renew the edge router's container, and only where it is
#                     running; the one flag accepted is --force-recreate, and it
#                     is required for the same reason. A line recorded as
#                     "protolab <flag>" by an earlier version means this step.
#   labels            the tehas labels are created in the product repository, or
#                     brought back to the colours and descriptions this version
#                     names; tehas labels is run with no arguments, and no
#                     arguments are accepted on the line.
#
# Rules: an id this file does not define is run by nobody, and it stays in the
# record so the host keeps reporting it until a command that knows it runs.
# Whether a step has happened is never read from what a command happened to copy
# - a step that was asked for and did not run stays owed until it has run.

# The systemd units install.sh names when a unit file changed. It asks for the
# timers and services tehas runs, not only the changed one: an update restarts
# the ones that were already active and leaves the rest as the operator left
# them, so naming all of them adds no work. A oneshot service whose timer is
# named here is not restarted, because restarting it would run the job off
# schedule; its changed definition reaches it when the units step asks systemd to
# reload.
tehas_units() { printf '%s\n' tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-clean.timer; }

tehas_step_ids() { printf '%s\n' units sandbox docs edge labels; }

# The units of tehas_units on one line, space separated, asked once per process:
# install.sh asks for every file it copies. Sourcing this file again, a newer
# copy of it, asks again.
_tehas_units_line=""
tehas_units_line() {
  [ -n "${_tehas_units_line:-}" ] || _tehas_units_line=$(tehas_units | tr '\n' ' ')
}

# The steps these changed sources call for, in the order they run: systemd
# first, because a unit file is not in effect until it is reloaded and its unit
# restarted, and the containers last.
tehas_steps_for() { # changed source paths -> one step per line
  tehas_steps_into "$@"
  if [ "${#tehas_steps[@]}" != 0 ]; then printf '%s\n' "${tehas_steps[@]}"; fi
  return 0
}

# The same answer in the array tehas_steps, with no subshell: install.sh asks
# once for every file it copies.
tehas_steps_into() { # changed source paths -> tehas_steps
  local list=" $* "
  tehas_steps=()
  case "$list" in
    # bin/tehas-energy is the sampler of the layout before bin/tehas. A running
    # one is that file calling itself, and it moves on only when restarted. A
    # running service keeps the environment env.sh gave it at its start.
    *" systemd/"* | *" bin/tehas-energy "* | *" env.sh "*)
      tehas_units_line
      tehas_steps+=("units ${_tehas_units_line% }")
      ;;
    # The MCP server's container mounts its binary: a new one reaches it only
    # when the unit starts the container again.
    *" libexec/mcp "* | *"/tehas-mcp "*) tehas_steps+=("units tehas-mcp.service") ;;
  esac
  case "$list" in
    *" sandbox/"* | *" versions.env "*) tehas_steps+=(sandbox) ;;
  esac
  case "$list" in
    # Both compose servers take --force-recreate: their service definition is
    # what changed only when the file they mount did, and docker compose leaves
    # a running container with an unchanged definition exactly as it was.
    *" edge/docs."*) tehas_steps+=("docs --force-recreate") ;;
  esac
  case "$list" in
    *" edge/router."* | *" edge/protolab."*) tehas_steps+=("edge --force-recreate") ;;
  esac
  case "$list" in
    # The command that makes the labels changed, so a release that adds one ends
    # with that label in place on GitHub rather than owed to nobody.
    *" libexec/labels "*) tehas_steps+=(labels) ;;
  esac
  return 0
}

# A step id as this file defines it. The Protolab's server became the edge
# router, so a line recorded as "protolab <flag>" by an earlier version means
# the edge step. One function says that, and everything that reads a step line
# asks it rather than spelling the alias out.
tehas_step_id() { # <id token> -> the id this file defines for it
  tehas_step_id_into "$1"
  printf '%s\n' "$tehas_step_id_out"
}
# The same answer in tehas_step_id_out, with no subshell.
tehas_step_id_into() {
  case "$1" in
    protolab) tehas_step_id_out=edge ;;
    *) tehas_step_id_out=$1 ;;
  esac
}

# The same step lines in the order they run, by id as tehas_step_ids lists them.
# Lines whose id this file does not know keep their order at the end: nobody runs
# them, but they stay visible. Sorting here rather than in each command keeps one
# answer to "what runs first" for install.sh's record and tehas update's run.
tehas_sort_steps() { # step lines -> the same lines, ordered
  local id line known tehas_step_id_out
  known=" $(tehas_step_ids | tr '\n' ' ') "
  for id in $known; do
    for line in "$@"; do
      tehas_step_id_into "${line%% *}"
      if [ "$tehas_step_id_out" = "$id" ]; then printf '%s\n' "$line"; fi
    done
  done
  for line in "$@"; do
    tehas_step_id_into "${line%% *}"
    case "$known" in
      *" $tehas_step_id_out "*) ;;
      *) printf '%s\n' "$line" ;;
    esac
  done
  return 0
}

# A systemd unit name: one word that ends in a unit suffix. A token that is not
# one is not a unit, whatever it looks like, and never reaches systemctl.
tehas_is_unit() {
  case "$1" in
    -*) return 1 ;;
  esac
  [[ $1 =~ ^[A-Za-z0-9_.:@+-]+\.(service|timer|socket)$ ]]
}

# Is this a step this file knows, carrying only arguments that id accepts? A
# line that fails here is not run: the command reading it says so out loud.
tehas_valid_step() { # <step line>
  local id token argv=()
  read -r -a argv <<<"$1"
  [ "${#argv[@]}" = 0 ] && return 1
  id=$(tehas_step_id "${argv[0]}")
  case "$id" in
    units)
      [ "${#argv[@]}" -gt 1 ] || return 1
      for token in "${argv[@]:1}"; do
        tehas_is_unit "$token" || return 1
        # And only the units this version of tehas installs. A step line is not a
        # way to name some other unit of the host's for an update to restart.
        tehas_units_line
        case " $_tehas_units_line " in
          *" $token "*) ;;
          *) return 1 ;;
        esac
      done
      ;;
    sandbox)
      [ "${#argv[@]}" = 1 ] || return 1
      ;;
    docs | edge)
      # Exactly one argument, and it is the flag: --force-recreate, or nothing.
      # Anything else - another option, a docker verb - is not this step. A bare
      # id is refused too, because docker compose up -d without the flag leaves a
      # running container serving the config it loaded at its start, and the line
      # would then be counted as a step that ran.
      [ "${#argv[@]}" = 2 ] || return 1
      [ "${argv[1]}" = --force-recreate ] || return 1
      ;;
    labels)
      # The command it runs takes no argument, so neither does the step.
      [ "${#argv[@]}" = 1 ] || return 1
      ;;
    *) return 1 ;;
  esac
  return 0
}

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

# Replace the record with these lines: what is owed after this run, in the order
# they run. No lines means no record. It writes through a temporary file beside
# it and moves that over, because the record is the only memory a half-finished
# run leaves behind - a truncation that died halfway would erase the steps that
# were already owed there.
tehas_write_steps() { # one step line per argument; none means remove the record
  local file tmp
  file=$(tehas_pending_file)
  if ! mkdir -p "${file%/*}"; then
    echo "tehas: could not create ${file%/*}" >&2
    return 1
  fi
  if [ "$#" = 0 ]; then
    rm -f "$file"
    return 0
  fi
  tmp=$(mktemp "$file.XXXXXX") || {
    echo "tehas: could not make a temporary file beside $file" >&2
    return 1
  }
  if ! printf '%s\n' "$@" >"$tmp" || ! mv -f "$tmp" "$file"; then
    rm -f "$tmp"
    echo "tehas: could not record the follow-ups owed in $file" >&2
    return 1
  fi
  return 0
}

# Add these steps to the record of steps owed, keeping it free of duplicates and
# in the order they run. install.sh calls it before each copy, so a run that
# dies partway still leaves behind what its copies asked for; tehas update calls
# it for the lines it was given and has not run. This writes, and says so when
# it cannot - including when what is already there cannot be read, because
# merging into an unreadable record would overwrite it.
tehas_record_steps() { # one step line per argument
  local line seen have sorted=() merged=() record
  if [ "$#" = 0 ]; then return 0; fi
  record=$(tehas_read_steps) || return 1
  while IFS= read -r line; do
    [ -n "$line" ] || continue
    merged+=("$line")
  done <<<"$record"
  for line in "$@"; do
    [ -n "$line" ] || continue
    seen=0
    for have in "${merged[@]}"; do
      if [ "$have" = "$line" ]; then seen=1; break; fi
    done
    if [ "$seen" = 0 ]; then merged+=("$line"); fi
  done
  if [ "${#merged[@]}" = 0 ]; then return 0; fi
  # A units step whose units another units step names too is done by that one.
  local kept=() unit covered
  for line in "${merged[@]}"; do
    covered=0
    if [ "${line%% *}" = units ]; then
      for have in "${merged[@]}"; do
        [ "${have%% *}" = units ] && [ "$have" != "$line" ] || continue
        covered=1
        for unit in ${line#units }; do
          case " ${have#units } " in *" $unit "*) ;; *) covered=0 ;; esac
        done
        [ "$covered" = 0 ] || break
      done
    fi
    [ "$covered" = 1 ] || kept+=("$line")
  done
  merged=("${kept[@]}")
  # Sorted as whole lines: a units step carries spaces, so it is read back one
  # line at a time rather than word-split.
  while IFS= read -r line; do sorted+=("$line"); done < <(tehas_sort_steps "${merged[@]}")
  # A record that already holds these lines in this order is left as it is.
  [ "$(printf '%s\n' "${sorted[@]}")" != "$record" ] || return 0
  tehas_write_steps "${sorted[@]}"
}
