# shellcheck shell=bash
# The snapshot behind `tehas top`, and the frame drawn from it.
#
#   tehas_top_snapshot <console lines wanted>    one JSON object on standard out
#   tehas_top_frame <snapshot> <width> <height>  the frame as plain text
#   tehas_sanitize_text                          text in, safe text out
#
# The snapshot reads what the factory already writes under $TEHAS_HOME/live/ -
# health.json, status.json, running.json, energy.json and the console of a
# cycle - and asks systemctl --user about its units. No GitHub call, no model,
# no Docker call, no instance settings, and nothing is written. A section whose
# source is missing, unreadable or too old to believe is null; the others are
# there all the same.
#
# The frame is drawn from the snapshot and from nothing else: it opens no file,
# asks no command for data and reads no clock. It cuts every line to the width
# it is given and never gives back more lines than the height it is given.
#
# Needs TEHAS_HOME and lib/status.sh beside it.

# shellcheck source=lib/status.sh
. "$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)/status.sh"

# A live file older than this is not believed, as the health file is not.
tehas_top_max_age=$tehas_health_max_age

# Control characters and escape sequences out of text that came from a file:
# tab is kept, everything else below 0x20 and 0x7f is dropped. Text that a
# file holds as a JSON escape is cleaned by tehas_top_safe_defs below, because
# it only becomes a control character once the JSON is read.
tehas_sanitize_text() {
  LC_ALL=C tr -d '\000-\010\013-\037\177'
}

# The same rule for every string of a JSON object, applied to the snapshot and
# to a snapshot handed to the renderer.
tehas_top_safe_defs='
  def safe:
    if type == "string" then (explode | map(select((. >= 32 and . != 127) or . == 9)) | implode)
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

tehas_top_snapshot() { # <console lines wanted>
  local want=${1:-16} live now health factory queue cycle units energy console
  local name issue ended file lines
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
    --argjson energy "$energy" --argjson console "$console" \
    "$tehas_top_safe_defs"'{ generatedAt: $generatedAt, factory: $factory, queue: $queue,
     cycle: $cycle, units: $units, energy: $energy, console: $console } | safe'
}

# The lines of a frame, one JSON item per line: {"p": <drop priority>, "kind":
# ..., ...}. Console lines are dropped first (1), then unit rows (2), then
# queue rows (3). Nothing else is dropped, and what is left fits the height.
tehas_top_plan() { # <snapshot> <width> <height>
  local snap=$1 cols=${2:-80} rows=${3:-24} now
  now=$(jq -r '.generatedAt | try fromdateiso8601 catch (now | floor)' <<<"$snap") || return 1
  jq -c --argjson cols "$cols" --argjson rows "$rows" --argjson now "$now" \
    "$tehas_top_safe_defs"'
    def cut($w): (. // "") | tostring | if length > $w then .[0:$w] else . end;
    def text($t; $p): {p: $p, n: 1, kind: "text", text: ($t | cut($cols))};
    def total: (map(.n // 1) | add) // 0;
    def drop($keys): to_entries | map(select(.key as $k | ($keys | index($k)) == null) | .value);
    def fit($h):
      if total <= $h then .
      else reduce range(1; 4) as $p (.;
        if total <= $h then .
        else (total - $h) as $need
          | ([to_entries[] | select(.value.p == $p) | .key] | reverse | .[:$need]) as $drop
          | drop($drop)
        end)
      end;

    def ago($e; $now): ($now - $e) | if . < 0 then 0 else . end
      | if . < 120 then "\(. | floor) s ago" elif . < 7200 then "\(. / 60 | floor) min ago"
        else "\(. / 3600 | floor) h ago" end;
    def stamp($t; $now): ($t | try fromdateiso8601 catch null) as $e
      | if $e == null then "" else " (" + ago($e; $now) + ")" end;

    . as $s
    | safe as $s
    | [ {p: 9, n: 1, kind: "text",
         text: (("tehas top   snapshot " + (($s.generatedAt // "") | sub("T"; " ") | sub("Z"; " UTC"))) | cut($cols))} ] as $title

    # 1. factory and the queue counts, in the words tehas status uses.
    | ( if $s.factory == null then [ text("factory   no health file yet"; 9) ]
        else [ {p: 9, n: 2, kind: "state", now: $now, stale: ($s.factory.stale // false),
                health: {status: $s.factory.status, time: $s.factory.time,
                         output: ($s.factory.output // ""), checks: {queue: ($s.queue.counts // null)}}} ]
        end ) as $factory

    # 2. the issues behind the counts.
    | [ ($s.queue.issues // [])[] | {p: 3, n: 1, kind: "issue", issue: .} ] as $issues

    # 3. the cycles.
    | ( if $s.cycle == null then [ text("cycle     no cycle written yet"; 9) ]
        else ($s.cycle.cycles // []) as $c
          | (if ($c | length) > 0 then $c else [ $s.cycle.last ] end) | map(select(. != null))
          | if length == 0 then [ {p: 9, n: 1, kind: "cycle", cycle: null, now: $now} ]
            else [ .[] | {p: 9, n: 1, kind: "cycle", cycle: ., now: $now} ] end
        end ) as $cycles

    # 4. the units of the factory.
    | ( if $s.units == null then [ text("units     systemctl --user gave no answer"; 9) ]
        elif ($s.units | length) == 0 then [ text("units     no tehas units"; 9) ]
        else [ text("units     \($s.units | length) units, \([$s.units[] | select(.activeState == "active")] | length) active"; 9) ]
             + [ $s.units[] | {p: 2, n: 1, kind: "unit", unit: .} ]
        end ) as $units

    # 5. what the model mesh draws.
    | [ if $s.energy == null then text("energy    no sample"; 9)
        else text("energy    \($s.energy.watts // "?") W now, \($s.energy.busy // "?") of \($s.energy.slots // "?") slots busy\(stamp($s.energy.time; $now))"; 9)
        end ] as $energy

    # 6. the console of a cycle, in the rows that are left.
    | ( if $s.console == null then [ text("console   no console file"; 9) ]
        else [ text("console   #\($s.console.issue // "?")  \($s.console.name)  \(if ($s.console.ended // false) then "ended" else "live" end)"; 9) ]
             + [ ($s.console.lines // [])[] | {p: 1, n: 1, kind: "console", line: .} ]
        end ) as $console

    | ($title + $factory + $issues + $cycles + $units + $energy + $console) | fit($rows) | .[]
  ' <<<"$snap"
}

# The frame as text. Everything it prints comes from the snapshot it is given.
tehas_top_frame() { # <snapshot> <width> <height>
  local snap=$1 cols=${2:-80} rows=${3:-24} plan item kind json now stale
  plan=$(tehas_top_plan "$snap" "$cols" "$rows") || return 1
  while IFS= read -r item; do
    kind=$(jq -r '.kind' <<<"$item") || return 1
    case "$kind" in
      text)
        jq -r '.text' <<<"$item"
        ;;
      state)
        json=$(jq -c '.health' <<<"$item")
        now=$(jq -r '.now' <<<"$item")
        stale=$(jq -r '.stale' <<<"$item")
        tehas_state_lines "$json" "$now" "$stale" | cut -c 1-"$cols"
        ;;
      cycle)
        json=$(jq -c '.cycle' <<<"$item")
        now=$(jq -r '.now' <<<"$item")
        tehas_cycle_line "$json" "$now" | cut -c 1-"$cols"
        ;;
      issue)
        jq -r --argjson cols "$cols" '
          "  " + ((.issue.number | tostring) + "     ")[0:5] +
          "  " + ((.issue.state // "") + "         ")[0:9] +
          "  " + (.issue.title // "") | .[0:$cols]
        ' <<<"$item"
        ;;
      unit)
        jq -r --argjson cols "$cols" '
          "  " + ((.unit.id // "") + "                          ")[0:26] +
          "  " + ((.unit.unitFileState // "-") | if . == "" then "-" else . end) +
          "  " + (.unit.activeState // "-") +
          ((.unit.next // "") | if . == "" then "" else "  next " + . end) | .[0:$cols]
        ' <<<"$item"
        ;;
      console)
        jq -r --argjson cols "$cols" '(.line // "") | .[0:$cols]' <<<"$item"
        ;;
    esac
  done <<<"$plan"
}
