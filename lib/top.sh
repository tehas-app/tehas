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
# The frame is drawn from the snapshot and from nothing else: it opens no file,
# asks no command for data and reads no clock. It cuts every line to the width
# it is given and never gives back more lines than the height it is given. The
# plan and the lines it draws are one jq program, so a frame with a long console
# tail costs the same one jq as a frame with two lines.
#
# Needs TEHAS_HOME and lib/status.sh beside it.

# shellcheck source=lib/status.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/status.sh"

# A live file older than this is not believed, as the health file is not.
tehas_top_max_age=$tehas_health_max_age

# The same rule for every string of a JSON object, applied to the snapshot and
# to a snapshot handed to the renderer. Code points 0x80 to 0x9f go; everything
# from 0xa0 up is text and stays.
tehas_top_safe_defs='
  def safe:
    if type == "string" then (explode | map(select((. >= 32 and . <= 126) or . == 9 or . >= 160)) | implode)
    elif type == "array" then map(safe)
    elif type == "object" then with_entries(.value |= safe)
    else . end;
'

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

# The frame as text, in one jq pass. The plan is built first, one item per line
# with a drop priority: {"p": ..., "kind": ...}. Host process rows are dropped
# first (1), then console lines and unit rows (2, the later row going first),
# then queue rows (3). Nothing else is dropped, and what is left fits the
# height. Every item is then drawn in the same pass, so a frame costs one jq
# whatever it holds: a long console tail is not one fork per line.
#
# The factory's two lines and the cycle line are the jq programs of
# lib/status.sh spliced into this one, so the frame and `tehas status` say the
# same thing because they run the same text.
# shellcheck disable=SC2016  # the jq program below is data, not a shell string
tehas_top_frame_jq='
    def cut($w): (. // "") | tostring | if length > $w then .[0:$w] else . end;
    def text($t; $p): {p: $p, n: 1, kind: "text", text: ($t | cut($cols))};
    def total: (map(.n // 1) | add) // 0;
    def drop($keys): to_entries | map(select(.key as $k | ($keys | index($k)) == null) | .value);
    def fit($h):
      if total <= $h then .
      else reduce range(1; 5) as $p (.;
        if total <= $h then .
        else (total - $h) as $need
          | ([to_entries[] | select(.value.p == $p) | .key] | reverse | .[:$need]) as $drop
          | drop($drop)
        end)
      end;

    def elapsed($e; $now): ($now - $e) | if . < 0 then 0 else . end
      | if . < 120 then "\(. | floor) s ago" elif . < 7200 then "\(. / 60 | floor) min ago"
        else "\(. / 3600 | floor) h ago" end;
    def stamp($t; $now): ($t | try fromdateiso8601 catch null) as $e
      | if $e == null then "" else " (" + elapsed($e; $now) + ")" end;

    # The two lines a health object gives, and one cycle line. $now and $stale
    # are the names those programs read, so they are passed as parameters.
    def state_lines($h; $now; $stale):
      ($h | 
'"$tehas_state_line_jq"'
      );
    def cycle_line($c; $now):
      ($c | 
'"$tehas_cycle_line_jq"'
      );

    def host_line:
      def q: if . == null then "?" else (. | tostring) end;
      # Three load averages or none: an absent reading is one "?", never a
      # line of separators with nothing between them.
      def three: if type == "array" and length == 3 then [ .[] | q ] | join(" ") else "?" end;
      "host      " + (.host.cpus | q) + " cpus, load " + (.host.load | three) +
      ", cpu " + (.host.cpuPercent | q) + "%, mem " +
      ((.host.memory.usedMb // "?") | tostring) + " of " + ((.host.memory.totalMb // "?") | tostring) + " MB";

    def process_line:
      def rj($v; $w): (($v // "?") | tostring) as $t
        | if ($t | length) >= $w then $t[0:$w] else ("          " + $t)[(-$w):] end;
      "  " + rj(.process.pid; 7) + "  " +
      (((.process.name // "") + "               ")[0:15]) + "  " +
      rj(.process.cpuPercent; 5) + "%  " + rj(.process.memPercent; 5) + "%  " +
      rj(.process.upSeconds; 8) + " s";

    def issue_line:
      "  " + ((.issue.number | tostring) + "     ")[0:5] +
      "  " + ((.issue.state // "") + "         ")[0:9] +
      "  " + (.issue.title // "");

    def unit_line:
      "  " + ((.unit.id // "") + "                          ")[0:26] +
      "  " + ((.unit.unitFileState // "-") | if . == "" then "-" else . end) +
      "  " + (.unit.activeState // "-") +
      ((.unit.next // "") | if . == "" then "" else "  next " + . end);

    def draw:
      if .kind == "text" then .text
      elif .kind == "state" then state_lines(.health; .now; .stale)
      elif .kind == "cycle" then cycle_line(.cycle; .now)
      elif .kind == "host" then host_line
      elif .kind == "process" then process_line
      elif .kind == "issue" then issue_line
      elif .kind == "unit" then unit_line
      else .line end | cut($cols);

    . as $s
    | safe as $s
    | ($s.generatedAt | try fromdateiso8601 catch (now | floor)) as $now
    | [ {p: 9, n: 1, kind: "text",
         text: (("tehas top   snapshot " + (($s.generatedAt // "") | sub("T"; " ") | sub("Z"; " UTC"))) | cut($cols))} ] as $title

    # 1. factory and the queue counts, in the words tehas status uses.
    | ( if $s.factory == null then [ text("factory   no health file yet"; 9) ]
        else [ {p: 9, n: 2, kind: "state", now: $now, stale: ($s.factory.stale // false),
                health: {status: $s.factory.status, time: $s.factory.time,
                         output: ($s.factory.output // ""), checks: {queue: ($s.queue.counts // null)}}} ]
        end ) as $factory

    # 2. the issues behind the counts.
    | [ ($s.queue.issues // [])[] | select(type == "object") | {p: 3, n: 1, kind: "issue", issue: .} ] as $issues

    # 3. the cycles.
    | ( if $s.cycle == null then [ text("cycle     no cycle written yet"; 9) ]
        else ($s.cycle.cycles // []) as $c
          | (if ($c | length) > 0 then $c else [ $s.cycle.last ] end) | map(select(type == "object"))
          | if length == 0 then [ {p: 9, n: 1, kind: "cycle", cycle: null, now: $now} ]
            else [ .[] | {p: 9, n: 1, kind: "cycle", cycle: ., now: $now} ] end
        end ) as $cycles

    # 4. the host: one line of load, cpu and memory, then the processes.
    # Process rows are the most expendable part of the frame: they go before
    # console lines and unit rows when the terminal is short.
    | ( if $s.host == null then [ text("host      no reading of /proc"; 9) ]
        else [ {p: 9, n: 1, kind: "host", host: $s.host} ]
             + [ ($s.host.processes // [])[] | select(type == "object") | {p: 1, n: 1, kind: "process", process: .} ]
        end ) as $host

    # 5. the units of the factory.
    | ( if $s.units == null then [ text("units     systemctl --user gave no answer"; 9) ]
        elif ($s.units | length) == 0 then [ text("units     no tehas units"; 9) ]
        else [ text("units     \($s.units | length) units, \([$s.units[] | select(.activeState == "active")] | length) active"; 9) ]
             + [ $s.units[] | select(type == "object") | {p: 2, n: 1, kind: "unit", unit: .} ]
        end ) as $units

    # 6. what the model mesh draws.
    | [ if $s.energy == null then text("energy    no sample"; 9)
        else text("energy    \($s.energy.watts // "?") W now, \($s.energy.busy // "?") of \($s.energy.slots // "?") slots busy\(stamp($s.energy.time; $now))"; 9)
        end ] as $energy

    # 7. the console of a cycle, in the rows that are left.
    | ( if $s.console == null then [ text("console   no console file"; 9) ]
        else [ text("console   #\($s.console.issue // "?")  \($s.console.name)  \(if ($s.console.ended // false) then "ended" else "live" end)"; 9) ]
             + [ ($s.console.lines // [])[] | {p: 2, n: 1, kind: "console", line: .} ]
        end ) as $console

    | ($title + $factory + $issues + $cycles + $host + $units + $energy + $console)
    | fit($rows)
    | .[]
    | draw
'

# The frame as text. Everything it prints comes from the snapshot it is given,
# and the whole of it - the plan, the dropping, and every line - is the one jq
# program above.
tehas_top_frame() { # <snapshot> <width> <height>
  local snap=$1 cols=${2:-80} rows=${3:-24}
  jq -r --argjson cols "$cols" --argjson rows "$rows" \
    "$tehas_top_safe_defs$tehas_state_defs$tehas_top_frame_jq" <<<"$snap" || return 1
}
