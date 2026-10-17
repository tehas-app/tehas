#!/usr/bin/env bash
# Tests for libexec/top-publish: the snapshot it writes for the dashboard.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/top_lib.sh
. "$(dirname "$0")/top_lib.sh"

published() { [ -f "$live/top.json" ] && echo there || echo gone; }
published_keys() { jq -r 'keys | join(",")' "$live/top.json"; }

t_case "the published file is the snapshot cut to the keys that may be published"
reset
files
energy
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$top --json"
snapshot=$t_out
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(published)" "there"
# The two snapshots are taken a second apart, so the age of the health file -
# and the moment stamped on the snapshot - differ by that second. Everything
# else is the same, and neither the console of a cycle nor the reading the host
# section was computed from is in the published file at all.
t_eq "$(jq -c 'del(.generatedAt, .console, .reading, .factory.ageSeconds)' "$live/top.json")" \
  "$(jq -c 'del(.generatedAt, .console, .reading, .factory.ageSeconds)' <<<"$snapshot")"
t_eq "$(jq -r '.host | type' "$live/top.json")" "object"
t_eq "$(published_keys)" "cycle,energy,factory,generatedAt,host,queue,units"
t_lacks "$live/top.json" "SecretTokenInACommandLine"

# What is published is a list of keys, not a list of keys to take out: two keys
# that are in the snapshot today reach the file only because they are named.
t_case "a key added to the snapshot later does not reach the published file"
reset
files
energy
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_eq "$(published_keys)" "cycle,energy,factory,generatedAt,host,queue,units"
# The same snapshot with a key no page draws, and one that holds a token: what
# comes out of the cut is what it was without them.
t_run '. "$T_ROOT/lib/top.sh"; snapshot=$(tehas_top_snapshot 16)
  extended=$(jq -c ". + {token: \"ghp_aStringThatIsASecret\", draft: {note: \"not published\"}}" <<<"$snapshot")
  tehas_top_published "$extended" >"$T/out"
  jq -c "del(.generatedAt, .factory.ageSeconds, .console, .reading)" <<<"$snapshot" >"$T/kept"'
t_eq "$t_rc" 0
t_eq "$(jq -r 'keys | join(",")' "$T/out")" "cycle,energy,factory,generatedAt,host,queue,units"
t_lacks "$T/out" "ghp_aStringThatIsASecret"
t_lacks "$T/out" "not published"
t_lacks "$T/out" "\"token\""
t_eq "$(cat "$T/kept")" "$(jq -c 'del(.generatedAt, .factory.ageSeconds)' "$T/out")"

# The cut answers nothing for a snapshot that is not one.
t_case "the published cut refuses a snapshot that is not an object"
reset
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_published "{not json}"'
t_eq "$t_rc" 1
t_run '. "$T_ROOT/lib/top.sh"; tehas_top_published ""'
t_eq "$t_rc" 1

t_case "the publisher writes the file and changes nothing else under live/"
reset
files
before=$(cd "$live" && cksum ./* | LC_ALL=C sort)
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$(cd "$live" && find . -maxdepth 1 -type f ! -name 'top.json' -printf '%f\n' | LC_ALL=C sort | tr '\n' ' ')" \
  "console-12.txt health.json running.json status.json "
t_contains "$(jq -c '.host.processes[].name' "$live/top.json")" "tehas-cycle"
t_eq "$(cd "$live" && cksum ./console-12.txt ./health.json ./running.json ./status.json | LC_ALL=C sort)" "$before"

t_case "TEHAS_TOP unset or 0 removes the file and exits 0"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$(published)" "there"
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=0
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$(published)" "gone"
t_instance "${T_MINIMAL[@]}"
t_run "$publish"
t_eq "$t_rc" 0
t_eq "$(published)" "gone"
# The setting governs the published file only: the terminal view works anyway.
t_run "$top --json | jq -r '.host | type'"
t_eq "$t_out" "object"

t_case "a TEHAS_TOP that is not 0 or 1 stops with 78 and leaves the file alone"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_run "$top --json"
snapshot=$t_out
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=yes
t_run "$publish"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_TOP is not valid: expected 0 or 1"
t_eq "$(jq -c 'del(.generatedAt, .console, .reading, .factory.ageSeconds)' "$live/top.json")" \
  "$(jq -c 'del(.generatedAt, .console, .reading, .factory.ageSeconds)' <<<"$snapshot")"

t_case "with no instance settings the publisher stops with 78 and writes nothing"
reset
files
mv "$TEHAS_INSTANCE_DIR/instance.env" "$T/instance.env.bak"
t_run "$publish"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env"
t_eq "$(published)" "gone"
mv "$T/instance.env.bak" "$TEHAS_INSTANCE_DIR/instance.env"

t_case "a file that cannot be written leaves the one that is there in place"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish"
t_eq "$t_rc" 0
keep=$(cat "$live/top.json")
# A directory where the temporary file goes: the write fails however the file
# modes are set, and the rename is never reached.
mkdir -p "$live/top.json.new"
t_run "$publish"
t_eq "$t_rc" 1
t_contains "$t_out" "could not write"
t_eq "$(cat "$live/top.json")" "$keep"
rmdir "$live/top.json.new"

t_case "with --every a malformed setting stops the publisher instead of looping"
reset
files
# The loop re-runs this script at every mark. Settings that are broken do not
# mend themselves between marks: the unit is told, so systemd shows it.
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=yes
t_run "$publish --every"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_TOP is not valid: expected 0 or 1"
t_eq "$(published)" "gone"

# The loop re-runs the script, and the re-run loads the settings again, which
# clears TEHAS_PROC_DIR from its own environment. What it reads has to be said
# to it, or a host section written in a loop would come from the machine that
# ran it rather than the one it was pointed at.
t_case "with --every the runs it starts read the host tree it was given"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
# One turn of the loop, stopped as soon as the file is there: what matters is
# which tree that run read, not how many snapshots it wrote. Its own output goes
# to a file, so nothing the loop leaves behind holds this one open.
t_run "$publish --every >\"$T/every.log\" 2>&1 & child=\$!; for _ in \$(seq 1 60); do [ -e \"$live/top.json\" ] && break; sleep 0.1; done; kill \$child 2>/dev/null; wait \$child 2>/dev/null; true"
t_eq "$(published)" "there"
t_eq "$(cat "$T/every.log")" ""
t_eq "$(jq -r '[.host.processes[].name] | sort | join(",")' "$live/top.json")" \
  "gh,pi-rukas,systemd,tehas-cycle"

t_case "the publisher takes no other argument"
reset
files
t_instance "${T_MINIMAL[@]}" TEHAS_TOP=1
t_run "$publish --watch"
t_eq "$t_rc" 64
t_contains "$t_out" "usage: tehasbot top [--every]"
t_eq "$(published)" "gone"

t_done
