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

# The state of one sample, as JSON text, from the reading the console makes and
# not from the file of api/: a sample that stopped printing its state is found
# here and not agreed with. Nothing when the reading holds no state.
sample_field_state() { # <surface> <name>
  local object
  object=$(sample_object "$1" "$2") || return 1
  [ -n "$object" ] || return 1
  printf '%s\n' "$object" | jq -c 'if has("state") then .state else "" | empty end'
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
  cases=()
  bounds=()
  required_json=
  while [ "$i" -lt "${#surfaces[@]}" ]; do
    line=${surfaces[i]}
    kind=$(surface_line "$line" 1)
    case $kind in
      sample) names+=("$(surface_line "$line" 2)") ;;
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
  if [ "$(jq -r '.properties | has("state")' "$schema")" != "true" ]; then
    [ "${#states[@]}" = 0 ] ||
      t_bad "the table names ${#states[@]} states for a schema of $surface with no state"
  else
    [ "${#names[@]}" = "${#states[@]}" ] ||
      t_bad "the table names ${#states[@]} states for ${#names[@]} samples of $surface"
    for n in "${!names[@]}"; do
      # The object of the reading comes from sample_object, which runs the console
      # and prints the object of the reading and nothing else; jq here picks the one
      # field out of it. Read from api/ the state would only be agreed with.
      printed_state=$(sample_field_state "$surface" "${names[$n]}") ||
        t_bad "the console printed no object for $surface ${names[$n]}"
      [ -n "$printed_state" ] ||
        t_bad "the reading of $surface ${names[$n]} holds no state"
      t_eq "$printed_state" "\"${states[$n]}\""
    done
    t_eq "$(jq -c '.properties.state.enum | sort' "$schema")" "$(printf '%s\n' "${states[@]}" | jq -R . | jq -sc 'sort')"
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
  [ -n "$held" ] || t_bad "no sample of $surface holds a field"
  t_eq "$(comm -23 <(printf '%s\n' "$paths") <(printf '%s\n' "$held"))" ""

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
    t_contains "$lines" ".$bound_field:"
    t_contains "$lines" "$word"
  done
done

t_done
