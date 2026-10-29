# shellcheck shell=bash
# The snapshot behind `tehas top`, and the frame drawn from it.
#
#   tehas_top_snapshot <lines> [<previous host reading>]  one JSON object on standard out
#   tehas_top_frame <snapshot> <width> <height>  the frame as plain text
#   tehas_top_published <snapshot>               the part of it that may be published
#
# The snapshot reads what the factory already writes under $TEHAS_HOME/live/ -
# health.json, status.json, running.json, energy.json and the console of a
# cycle - asks systemctl --user about its units - and reads the host itself in
# /proc, through the console beside the commands (console/src/top.rs and
# console/src/snapshot.rs). No GitHub call, no model, no Docker call, no instance settings, and
# nothing is written. A section whose source is missing, unreadable or too old
# to believe is null; the others are there all the same.
#
# The host section compares two readings of /proc. Given the reading taken by
# the call before it, the snapshot takes one reading and waits for nothing: the
# difference it reports is the time since that call. Given none, it takes a
# second reading a second later so that its CPU figures measure something. The
# reading a snapshot was computed from is in the snapshot under `reading`, which
# is how a view that redraws hands it to the next frame. No page draws it, and
# tehas_top_published leaves it out of what a page may read.
#
# The host section names a process by its short command name (comm) only. The
# command line is never opened: it can hold tokens, addresses and paths. No
# user name, working directory, environment, host name, address or open file is
# read either, for the terminal and the published file alike.
#
# The frame is drawn from the snapshot and from nothing else, by the console
# (console/src/frame.rs): it opens no file, asks no command for data and reads
# no clock. It cuts every line to the width it is given and never gives back
# more lines than the height it is given.
#
# Needs TEHAS_HOME.

# A live file older than this is not believed, as the health file is not
# (console/src/status.rs).
tehas_top_max_age=600

tehas_top_snapshot() { # <console lines wanted> [<previous host reading>]
  local want=${1:-16} was=${2:-} reading later
  [ -x "$tehas_top_console" ] ||
    { echo "tehas top: no console at $tehas_top_console, run install.sh" >&2; return 1; }
  # The host itself: a reading now, measured against the one before it. With a
  # previous reading there is nothing to wait for and the window is the time
  # since that reading. Without one, take a second reading a second on.
  if [ -n "$was" ] && ! jq -e 'type == "object"' <<<"$was" >/dev/null 2>&1; then was=''; fi
  reading=$(tehas_host_reading) || reading=''
  if [ -n "$reading" ] && [ -z "$was" ]; then
    sleep 1
    later=$(tehas_host_reading) || later=''
    if [ -n "$later" ]; then
      was=$reading
      reading=$later
    fi
  fi
  # The rest of the snapshot is read and put together by the console
  # (console/src/snapshot.rs).
  "$tehas_top_console" top snapshot "$TEHAS_HOME/live" "$want" "$tehas_top_max_age" "$reading" "$was"
}

# --- the host section -------------------------------------------------------

# Where the host's own numbers are read. On a host this is /proc; the tests
# point it at a tree of files they write, so no case depends on the machine
# that runs it. Nothing outside this directory is opened.
tehas_proc_dir=${TEHAS_PROC_DIR:-/proc}

# The console that reads the host and computes its section
# (console/src/top.rs): the one beside the commands, or TEHAS_CONSOLE for a
# tree that has none, read when this is sourced as TEHAS_PROC_DIR is.
tehas_top_console=${TEHAS_CONSOLE:-$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/../libexec/console}

# One reading of the host as one line of JSON; nothing when it cannot be read.
tehas_host_reading() { # [dir]
  "$tehas_top_console" top reading "${1:-$tehas_proc_dir}" 2>/dev/null
}

# The host section from two readings: the later one and the earlier one.
tehas_top_host() { # <later reading> <earlier reading>
  [ -n "${1:-}" ] && [ -n "${2:-}" ] || return 1
  "$tehas_top_console" top host "$1" "$2" 2>/dev/null
}

# What of the snapshot may be published: these keys and nothing else. A key
# added to the snapshot later stays out of live/top.json until someone names it
# here - the dashboard can be on the internet, and a rule that only removes the
# keys somebody thought of is not a rule. The console of a cycle and the reading
# a host section was computed from are in the snapshot and not in this list.
# What is inside a listed key is fixed by the code that builds it: the host
# section and the units name every field they give (console/src/top.rs and
# console/src/snapshot.rs), and the queue, cycle, energy
# and console sections are cut to the fields the frame draws.
tehas_top_publish_keys='["generatedAt","factory","queue","cycle","host","units","energy"]'

tehas_top_published() { # <snapshot> -> the part of it that may be published, one line
  jq -e 'type == "object"' <<<"${1:-}" >/dev/null 2>&1 || return 1
  jq -c --argjson keep "$tehas_top_publish_keys" \
    'with_entries(select(.key | IN($keep[])))' <<<"$1"
}

tehas_top_frame() { # <snapshot> <width> <height>
  "$tehas_top_console" top frame "${2:-80}" "${3:-24}" <<<"$1"
}
