# shellcheck shell=bash
# The snapshot behind `tehas top`, and the frame drawn from it.
#
#   tehas_top_snapshot <lines> [<previous host reading>]  one JSON object on standard out
#   tehas_top_frame <snapshot> <width> <height>  the frame as plain text
#   tehas_top_published <snapshot>               the part of it that may be published
#   tehas_sanitize_text                          text in, safe text out
#
# The snapshot reads what the factory already writes under $TEHAS_HOME/live/ -
# health.json, status.json, running.json, energy.json and the console of a
# cycle - asks systemctl --user about its units - and reads the host itself in
# /proc. No GitHub call, no model, no Docker call, no instance settings, and
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

# Control characters and escape sequences out of text that came from a file:
# tab is kept, everything else below 0x20 and 0x7f is dropped, and so are the
# C1 controls from 0x80 to 0x9f, which some terminals read as the start of a
# control sequence. A C1 byte is dropped only where it is the second byte of
# its own UTF-8 encoding (0xc2 followed by 0x80 to 0x9f): the same byte value
# is also a continuation byte of a character that is not a control, and cutting
# that out would break a title written in any language.
#
# Text that a file holds as a JSON escape is cleaned by tehas_top_safe_defs
# below, because it only becomes a control character once the JSON is read.
tehas_sanitize_text() {
  LC_ALL=C tr -d '\000-\010\013-\037\177' |
    LC_ALL=C sed 's/\xc2[\x80-\x9f]//g'
}

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

# The units section: every tehas-* timer and service of the user's systemd, as
# a JSON array. An empty array when there are none; nothing on standard out
# when systemd cannot be asked at all (no user bus, another account).
tehas_top_units() {
  local units='' listed='' props line
  local -a names=() uniq=()
  local rc_units=0 rc_files=0
  units=$(systemctl --user list-units --all --full --plain --no-legend 'tehas-*' 2>/dev/null) || rc_units=$?
  listed=$(systemctl --user list-unit-files --no-legend 'tehas-*' 2>/dev/null) || rc_files=$?
  # Neither call answered at all: there is no user bus to ask.
  if [ "$rc_units" != 0 ] && [ "$rc_files" != 0 ]; then return 1; fi
  while IFS= read -r line; do
    [ -n "$line" ] && names+=("$line")
  done < <(printf '%s\n%s\n' "$units" "$listed" | awk 'NF { print $1 }')
  if [ "${#names[@]}" = 0 ]; then echo '[]'; return 0; fi
  mapfile -t uniq < <(printf '%s\n' "${names[@]}" | LC_ALL=C sort -u)
  # One block of Key=Value lines per unit. A new Id= begins the next block, so
  # this reads them whether or not systemd puts a blank line between blocks.
  props=$(systemctl --user show -p Id,LoadState,ActiveState,SubState,UnitFileState,NextElapseUSecRealtime \
    "${uniq[@]}" 2>/dev/null) || return 1
  [ -n "$props" ] || { echo '[]'; return 0; }
  printf '%s\n' "$props" | awk '
    function emit() {
      if (id == "") return
      printf "{\"id\":\"%s\",\"loadState\":\"%s\",\"activeState\":\"%s\",\"subState\":\"%s\",\"unitFileState\":\"%s\",\"next\":\"%s\"}\n",
        id, p["LoadState"], p["ActiveState"], p["SubState"], p["UnitFileState"], clock
    }
    {
      e = index($0, "=")
      if (e == 0) next
      k = substr($0, 1, e - 1); v = substr($0, e + 1)
      if (k == "Id") { emit(); id = v; delete p; clock = "" }
      p[k] = v
      if (k == "NextElapseUSecRealtime" && match(v, /[0-9][0-9]:[0-9][0-9]:[0-9][0-9]/)) {
        clock = substr(v, RSTART, RLENGTH)
      }
    }
    END { emit() }
  ' | jq -c -s '.'
}

tehas_top_snapshot() { # <console lines wanted> [<previous host reading>]
  local want=${1:-16} live now health factory queue cycle units energy console host
  local name issue ended file lines was reading later
  live=$TEHAS_HOME/live
  now=$(date +%s)

  # Factory and queue counts: the health file, with the rule tehas status uses.
  health=null
  if tehas_health_readable "$live/health.json"; then
    health=$(tehas_sanitize_text <"$live/health.json" |
      jq -c '{status, time, output, checks: {queue: ((.checks // {}).queue)}}' 2>/dev/null) || health=null
    case "$health" in '' | null) health=null ;; esac
  fi
  if [ "$health" != null ]; then
    factory=$(jq -c --argjson now "$now" --argjson max "$tehas_top_max_age" '
      . + {ageSeconds: ($now - (.time | try fromdateiso8601 catch $now)),
           stale: (($now - (.time | try fromdateiso8601 catch 0)) > $max)}' <<<"$health") || factory=null
  else
    factory=null
  fi

  # The issues behind the counts, from the queue snapshot itself. Running ones
  # first, then in line, in review and stopped.
  queue=null
  if jq -e 'type == "object"' "$live/status.json" >/dev/null 2>&1; then
    queue=$(tehas_sanitize_text <"$live/status.json" |
      jq -c --argjson counts "$(if [ "$health" = null ]; then echo null; else jq -c '.checks.queue // null' <<<"$health"; fi)" '
      {time: (.generatedAt // null),
       counts: $counts,
       issues: ([ (.running // [])[]  | {number, state: "running", title} ] +
                [ (.queued // [])[]   | {number, state: "in line", title} ] +
                [ (.awaiting // [])[] | {number, state: "in review", title} ] +
                [ (.stopped // [])[]  | {number, state: "stopped", title} ])}
    ' 2>/dev/null) || queue=null
    case "$queue" in '' | null) queue=null ;; esac
  fi

  # The cycles, in the shape health.json gives them: times as ISO text.
  cycle=null
  if jq -e 'type == "object"' "$live/running.json" >/dev/null 2>&1; then
    cycle=$(tehas_sanitize_text <"$live/running.json" | jq -c '
      def iso: if type == "number" then todate else null end;
      def brief: {issue, running: (.running // false), status, step, console,
                  startedAt: (.startedAt | iso), time: (.updatedAt | iso)};
      (if has("cycles") then .
       elif .running == true then {cycles: [.], last: null}
       else {cycles: [], last: .} end) as $v
      | {cycles: (($v.cycles // []) | map(brief)),
         last: ($v.last | if . == null then null else brief end)}
    ' 2>/dev/null) || cycle=null
    case "$cycle" in '' | null) cycle=null ;; esac
  fi

  # The host itself: a reading now, measured against the one before it. With a
  # previous reading there is nothing to wait for and the window is the time
  # since that reading. Without one, take a second reading a second on.
  was=${2:-}
  if [ -n "$was" ] && ! jq -e 'type == "object"' <<<"$was" >/dev/null 2>&1; then was=''; fi
  host=''
  reading=$(tehas_host_reading) || reading=''
  if [ -n "$reading" ] && [ -n "$was" ]; then
    host=$(tehas_top_host "$reading" "$was") || host=''
  elif [ -n "$reading" ]; then
    sleep 1
    later=$(tehas_host_reading) || later=''
    if [ -n "$later" ]; then
      was=$reading
      reading=$later
      host=$(tehas_top_host "$reading" "$was") || host=''
    fi
  fi
  case "$host" in '' | null) host=null ;; esac
  case "$reading" in '' | null) reading=null ;; esac

  units=$(tehas_top_units) || units=null
  case "$units" in '' | null) units=null ;; esac
  if [ "$units" != null ] && ! jq -e 'type == "array"' <<<"$units" >/dev/null 2>&1; then
    units=null
  fi

  energy=null
  if jq -e 'type == "object"' "$live/energy.json" >/dev/null 2>&1; then
    energy=$(jq -c --argjson now "$now" --argjson max "$tehas_top_max_age" '
      select((.generatedAt | try fromdateiso8601 catch 0) > $now - $max)
      | {time: .generatedAt, watts: (.totals.watts // null), hosts: (.totals.hosts // null),
         slots: ([.models[]?.slots | select(type == "number")] | add),
         busy: ([.models[]?.busy | select(type == "number")] | add)}
    ' "$live/energy.json" 2>/dev/null) || energy=null
    case "$energy" in '' | null) energy=null ;; esac
  fi

  # The console of a running cycle, or of the one that ended last.
  console=null
  name=$(jq -r '(if ((.cycles // []) | length) > 0 then .cycles[-1] else .last end) // null
                | if . == null then "" else (.console // "console-\(.issue).txt") end' <<<"$cycle" 2>/dev/null)
  issue=$(jq -r '(if ((.cycles // []) | length) > 0 then .cycles[-1].issue else .last.issue end) // null' <<<"$cycle" 2>/dev/null)
  ended=true
  jq -e '((.cycles // []) | length) > 0' <<<"$cycle" >/dev/null 2>&1 && ended=false
  # The name came out of a file, and the file it names is opened. Only a plain
  # name is opened as one: a name that holds a path, or is . or .., or holds a
  # character outside the set a cycle writes, reads nothing, and the frame says
  # there is no console file.
  case "$name" in
    '' | .* | *..* | *[!A-Za-z0-9._-]*) name='' ;;
  esac
  file=$live/${name:-}
  if [ -n "$name" ] && [ -s "$file" ]; then
    lines=$(tail -n "$want" "$file" 2>/dev/null | tehas_sanitize_text)
    console=$(jq -Rs --arg name "$name" --arg issue "$issue" --argjson ended "$ended" '
      split("\n") | map(select(. != "")) | {issue: $issue, name: $name, ended: $ended, lines: .}' <<<"$lines") ||
      console=null
    case "$console" in '' | null) console=null ;; esac
  fi

  jq -c -n --arg generatedAt "$(date -u +%FT%TZ)" --argjson factory "$factory" \
    --argjson queue "$queue" --argjson cycle "$cycle" --argjson units "$units" \
    --argjson energy "$energy" --argjson console "$console" --argjson host "$host" \
    --argjson reading "$reading" \
    "$tehas_top_safe_defs"'{ generatedAt: $generatedAt, factory: $factory, queue: $queue,
     cycle: $cycle, host: $host, units: $units, energy: $energy, console: $console,
     reading: $reading } | safe'
}

# --- the host section -------------------------------------------------------

# Where the host's own numbers are read. On a host this is /proc; the tests
# point it at a tree of files they write, so no case depends on the machine
# that runs it. Nothing outside this directory is opened.
tehas_proc_dir=${TEHAS_PROC_DIR:-/proc}

# How many processes the section keeps, and the bytes in one page of resident
# memory, which /proc counts in pages.
tehas_host_rows=15
tehas_page_size=$(getconf PAGE_SIZE 2>/dev/null) || tehas_page_size=''
case "$tehas_page_size" in '' | *[!0-9]*) tehas_page_size=4096 ;; esac

# One reading of the host, as JSON: the clock in seconds (/proc/uptime), the
# number of CPUs, the load averages, memory in KiB, the aggregate cpu line of
# /proc/stat, and one row per process: pid, comm, CPU ticks, start ticks and
# resident pages. A tick is a hundredth of a second on every Linux this reads.
# The clock is the host's own, so two readings measure a difference without
# either of them looking at a wall clock or a sleep.
tehas_host_reading() { # [dir] -> the reading as JSON; nothing when it cannot be read
  local dir pid row line up n procs mem k v root
  local -a f load=() files=()
  local total='' idle='' memtotal='' memavail=''
  root=${1:-$tehas_proc_dir}
  [ -r "$root/uptime" ] && [ -r "$root/stat" ] || return 1
  IFS= read -r up <"$root/uptime" || return 1
  up=${up%% *}
  case "$up" in '' | *[!0-9.]*) return 1 ;; esac

  # The first line of /proc/stat is the sum over every cpu: user nice system
  # idle iowait irq softirq steal. Idle and iowait are not use.
  IFS= read -r line <"$root/stat" || return 1
  read -r -a f <<<"$line" || return 1
  [ "${f[0]:-}" = cpu ] || return 1
  [ "${#f[@]}" -ge 9 ] || return 1
  total=0
  for row in "${f[@]:1:8}"; do
    case "$row" in '' | *[!0-9]*) return 1 ;; esac
    total=$((total + row))
  done
  idle=$((f[4] + f[5]))

  # The CPUs of this host: the processors /proc/cpuinfo names. Where it names
  # none, ask the machine.
  n=0
  if [ -r "$root/cpuinfo" ]; then
    while IFS= read -r line; do
      case "$line" in processor*) n=$((n + 1)) ;; esac
    done <"$root/cpuinfo"
  fi
  if [ "$n" = 0 ]; then
    n=$(nproc 2>/dev/null) || n=''
    case "$n" in '' | *[!0-9]*) n=1 ;; esac
  fi

  # The three load averages and memory in KiB. Either can be absent, and the
  # section then says so with null rather than guessing at it.
  load=()
  if [ -r "$root/loadavg" ] && IFS= read -r line <"$root/loadavg"; then
    read -r -a f <<<"$line" || true
    for row in "${f[@]:0:3}"; do
      case "$row" in '' | *[!0-9.]*) load=(); break ;; esac
      load+=("$row")
    done
    [ "${#load[@]}" = 3 ] || load=()
  fi
  if [ -r "$root/meminfo" ]; then
    while read -r k v _; do
      case "$k" in
        MemTotal:) memtotal=$v ;;
        MemAvailable:) memavail=$v ;;
      esac
    done <"$root/meminfo"
  fi
  case "$memtotal" in '' | *[!0-9]*) memtotal='' ;; esac
  case "$memavail" in '' | *[!0-9]*) memavail='' ;; esac
  if [ -n "$memtotal" ] && [ -n "$memavail" ]; then
    mem=$(printf '{"totalKb":%s,"availKb":%s}' "$memtotal" "$memavail")
  else
    mem=null
  fi

  # Every process in one pass. A fork for each pid costs more than the reading
  # itself on a host with hundreds of processes, and two readings go into every
  # snapshot of a service that runs for ever.
  files=()
  for dir in "$root"/*; do
    pid=${dir##*/}
    case "$pid" in '' | *[!0-9]*) continue ;; esac
    if [ -r "$dir/stat" ]; then files+=("$dir"); fi
  done

  procs='[]'
  if [ "${#files[@]}" -gt 0 ]; then
    procs=$(printf '%s\n' "${files[@]}" | _tehas_proc_rows) || procs=''
    case "$procs" in '' | '['*']') ;; *) procs='[]' ;; esac
  fi

  printf '{"clock":%s,"cpus":%s,"load":[%s],"memory":%s,"cpuAll":%s,"cpuIdle":%s,"procs":%s}\n' \
    "$up" "$n" "${load:+$(printf '%s,' "${load[@]}" | LC_ALL=C sed 's/,$//')}" \
    "$mem" "$total" "$idle" "$procs"
}

# The rows of one reading as a JSON array: [pid,comm,cpu ticks,start ticks,rss
# pages] for every directory named on stdin. awk reads the stat files, so one
# process does the work of what was a fork per pid, and jq turns the fields into
# JSON, so a name is escaped where it is read rather than by string surgery.
#
# The comm field of /proc/<pid>/stat is text between brackets that may itself
# hold a space or a bracket, so it is what lies between the first ( and the last
# ), and the fields after it are counted from there: state comes first, so utime
# is the twelfth of those, starttime the twentieth and rss the twenty-second. A
# file that cannot be read, or that names too few fields, gives no row.
_tehas_proc_rows() { # <process directory>..., one per line on stdin
  LC_ALL=C awk '
    function row(dir,   parts, n, pid, s, line, f, k) {
      n = split(dir, parts, "/")
      pid = parts[n]
      s = ""
      while ((getline line < (dir "/stat")) > 0) s = s (s == "" ? "" : " ") line
      close(dir "/stat")
      if (s == "") return            # unreadable, or gone since it was listed
      gsub(/[[:cntrl:]]/, " ", s)
      if (!match(s, /[(].*[)]/)) return
      k = split(substr(s, RSTART + RLENGTH), f, " ")
      if (k < 22) return
      if (f[12] !~ /^[0-9]+$/ || f[13] !~ /^[0-9]+$/) return
      if (f[20] !~ /^[0-9]+$/ || f[22] !~ /^[0-9]+$/) return
      # utime and stime are added by jq; here they stay the text of the file.
      print pid "\t" substr(s, RSTART + 1, RLENGTH - 2) "\t" f[12] "\t" f[13] \
            "\t" f[20] "\t" f[22]
    }
    { row($0) }
  ' | jq -cRn '
    [ inputs | select(. != "") | split("\t") | select(length == 6)
      | [ (.[0] | tonumber), .[1],
          ((.[2] | tonumber) + (.[3] | tonumber)),
          (.[4] | tonumber), (.[5] | tonumber) ] ]
  '
}

# The host section from two readings: the later one and the earlier one. CPU
# use is what changed between them - never the average since a process started.
# A process counts in the difference only when it has the same pid and the same
# start time in both, so a reused pid is not read as one long process. Two
# readings at the same instant measure nothing: their CPU figures are null.
tehas_top_host() { # <later reading> <earlier reading>
  local now=$1 was=$2
  [ -n "$now" ] && [ -n "$was" ] || return 1
  jq -c -n --argjson now "$now" --argjson was "$was" --argjson rows "$tehas_host_rows" \
    --argjson page "$tehas_page_size" '
    def num: if type == "number" and . >= 0 then . else null end;
    def one: (. * 10 | round) / 10;
    def clamp($lo; $hi): if . < $lo then $lo elif . > $hi then $hi else . end;
    def row5: select(type == "array" and length == 5);
    (if ($now | type) == "object" then $now else null end) as $a
    | (if ($was | type) == "object" then $was else null end) as $b
    | if $a == null or $b == null then null else
        ($a.clock | num) as $c2 | ($b.clock | num) as $c1
        | (if $c2 != null and $c1 != null and $c2 > $c1 then (($c2 - $c1) * 100 | round) else null end) as $dt
        | ($b.procs // [] | map(row5 | {key: (.[0] | tostring), value: .}) | from_entries) as $prev
        | ($a.cpuAll | num) as $t2 | ($b.cpuAll | num) as $t1
        | ($a.cpuIdle | num) as $i2 | ($b.cpuIdle | num) as $i1
        | (if $t2 != null and $t1 != null then $t2 - $t1 else null end) as $dall
        | (if $i2 != null and $i1 != null then $i2 - $i1 else null end) as $didle
        | ($a.memory // null) as $mem
        | (if $mem == null then null
           else { totalKb: ($mem.totalKb | num), availKb: ($mem.availKb | num) } end) as $m
        | {
            cpus: ($a.cpus | num),
            load: ($a.load // null | if type == "array" and length == 3 then . else null end),
            cpuPercent: (if $dt != null and $dall != null and $dall > 0 and $didle != null
                         then ((($dall - $didle) / $dall * 100) | clamp(0; 100) | one)
                         else null end),
            memory: (if $m == null or $m.totalKb == null or $m.availKb == null or $m.totalKb <= 0 then null
                     else { totalMb: ($m.totalKb / 1024 | round),
                            usedMb: (($m.totalKb - $m.availKb) / 1024 | round),
                            percent: ((($m.totalKb - $m.availKb) / $m.totalKb * 100) | clamp(0; 100) | one) } end),
            processes: ([ $a.procs // [] | .[] | row5
                          | . as $r | $prev[(.[0] | tostring)] as $p
                          | select($p != null and ($p[3] | num) == (.[3] | num))
                          | { pid: .[0], name: (.[1] // ""),
                              cpuPercent: (if $dt != null and $dt > 0 and ((.[2] | num) != null) and (($p[2] | num) != null)
                                           then (((.[2] | num) - ($p[2] | num)) / $dt * 100 | clamp(0; 99999) | one)
                                           else null end),
                              memPercent: (if $m == null or $m.totalKb == null or $m.totalKb <= 0 or ((.[4] | num) == null)
                                           then null
                                           else ((.[4] * $page / 1024) / $m.totalKb * 100 | clamp(0; 100) | one) end),
                              upSeconds: (if $c2 == null or ((.[3] | num) == null)
                                          then null
                                          else ((($c2 * 100 | round) - .[3]) / 100 | floor | clamp(0; 999999999)) end) }
                        ]
                        | sort_by([ -(.cpuPercent // -1), .pid ])
                        | .[0:$rows])
          }
      end
    '
}

# What of the snapshot may be published: these keys and nothing else. A key
# added to the snapshot later stays out of live/top.json until someone names it
# here - the dashboard can be on the internet, and a rule that only removes the
# keys somebody thought of is not a rule. The console of a cycle and the reading
# a host section was computed from are in the snapshot and not in this list.
# What is inside a listed key is fixed by the code that builds it: tehas_top_host
# and tehas_top_units name every field they give, and the queue, cycle, energy
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
