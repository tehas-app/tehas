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
  'check	del(.state) | .zz = 1 | .cycles = "1"	.: state is missing'
  'check	del(.state) | .zz = 1 | .cycles = "1"	.zz: not in the schema'
  'check	del(.state) | .zz = 1 | .cycles = "1"	.cycles: string, not integer'
  'required	["api","state"]'
)

# The limit of the `limit` samples, as it is set: one cycle at a time, no
# planning, an expiry forty minutes ahead and a reason. `expired` is the same
# file read later, so a `--until` in the past is never given: console flags
# refuses one with 64, at the moment it is set at. The reading of the file, not
# the writing of it, is what a sample is, so `set` prints its words and not
# `--json`: as the last argument of set, `--json` is refused as an option
# without its value.
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

# The moment the limit file is written at: 2026-10-17T14:00:20Z, when it is in
# force, whichever reading of it a sample is. `set` takes the same moment for
# its own now, so it never sees a `--until` of the past. Named here so the two
# places that give it to a command say the same thing.
limit_written=1792245620

# <name>: the state of the world that sample is a reading of: the limit file,
# or a directory where it would be. Each run of the console goes through t_run,
# which builds its environment from nothing: HOME and TEHAS_HOME of the run's
# own, so the file lands under the scratch tree and nowhere else. The clock of
# the run that writes the file goes in the snippet itself, since an assignment
# before the call would not reach a command t_run builds with env -i.
limit_world() { # <name>
  case $1 in
    sample) t_run "TEHAS_FLAGS_TEST_NOW=$limit_written '$T_CONSOLE' \
        flags limit set ${limit_set[*]}" ||
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

# <surface> <name>: the object the code prints for that sample, as committed,
# on standard output. The console of this run prints it, and the same program
# wrote the file. Not bin/tehas: that goes through console front, whose load of
# the instance drops every TEHAS_ name the settings hold no value for, the test
# clock among them, so nothing can be read at a moment of its own through that
# door.
printed() { # <surface> <name>
  local surface=$1 name=$2
  rm -rf "$T/home/tehas"
  t_instance "${T_MINIMAL[@]}"
  case $surface in
    limit)
      limit_world "$name"
      limit_reading "$name"
      printf '%s\n' "$t_out"
      ;;
    *) return 1 ;;
  esac
}

# The `required` of the surface, as the table's `required` line gives it.
required_json=

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
  cases=()
  required_json=
  while [ "$i" -lt "${#surfaces[@]}" ]; do
    line=${surfaces[i]}
    kind=$(surface_line "$line" 1)
    case $kind in
      sample) names+=("$(surface_line "$line" 2)") ;;
      required) required_json=$(surface_line "$line" 2) ;;
      check)
        object=$(surface_line "$line" 2)
        cases+=("$object"$'\t'"$(surface_line "$line" 3)")
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
  # Every keyword of the schema is one the check looks at, so none is passed
  # over in silence: the keys of every object that holds fields of its own are
  # a keyword of the list test/schema.jq knows.
  t_eq "$(jq -c '[.. | objects | select(has("properties") or has("const") or
      has("enum") or has("type")) | keys_unsorted[] | select(IN("$schema", "$id",
        "title", "description", "type", "const", "enum", "required",
        "additionalProperties", "properties", "items", "minimum", "maximum",
        "minLength", "maxLength", "pattern", "maxItems", "format") | not)]' "$schema")" "[]"
  t_contains "$(jq -r '.description' "$schema")" "test/json_contract_test.sh"

  if [ -n "$samples_write" ]; then
    for name in "${names[@]}"; do
      printed "$surface" "$name" >"$api/$surface.$name.json"
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

  t_case "the samples of $surface between them hold every field of its schema"
  paths=$(schema_paths "$schema") || t_bad "the schema could not be walked"
  # A schema whose fields cannot be named would compare equal to a walk of
  # nothing, so both sides of the comparison are said here when either is empty.
  [ -n "$paths" ] || t_bad "the schema of $surface names no field"
  held=$(for name in "${names[@]}"; do held_paths "$api/$surface.$name.json"; done | sort -u)
  case $held in *"could not be read"*) t_bad "a sample of $surface holds no object:$held" ;; esac
  [ -n "$held" ] || t_bad "no sample of $surface holds a field"
  t_eq "$(comm -23 <(printf '%s\n' "$paths") <(printf '%s\n' "$held"))" ""

  t_case "the manual names every field of the schema of $surface"
  manual=$(cat "$T_ROOT/host/docs/api.md")
  unnamed=""
  for path in $paths; do
    case $manual in *"| \`$path\` |"*) ;; *) unnamed+=" $path" ;; esac
  done
  t_eq "$unnamed" ""

  t_case "the check finds a field of $surface missing, one of the wrong type, and one it does not know"
  for case_line in "${cases[@]}"; do
    if ! jq "${case_line%%$'\t'*}" "$api/$surface.sample.json" >"$T/bad.json"; then
      t_bad "the jq of the table's line is no jq: ${case_line%%$'\t'*}"
      continue
    fi
    lines=$(check "$schema" "$T/bad.json") || t_bad "the schema check could not be run"
    t_contains "$lines" "${case_line#*$'\t'}"
  done
done

t_done
