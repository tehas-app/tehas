#!/usr/bin/env bash
# Tests for lib/update-steps.sh: the follow-up names install.sh prints and
# tehas update runs, the sources that call for each one, the order the steps run
# in, and the record of steps owed. What counts as a step line rather than text
# is the console's: its cases are the Rust tests of console/src/update/steps.rs.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=lib/update-steps.sh
. "$T_ROOT/lib/update-steps.sh"
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"

t_case "the changed sources ask for the steps, systemd first"
t_eq "$(tehas_steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env \
  edge/docs.compose.yaml edge/router.compose.yaml libexec/dispatch)" \
  "units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer
sandbox
docs --force-recreate
edge --force-recreate"

t_case "a new MCP server restarts its unit, and only it"
t_eq "$(tehas_steps_for /src/console/target/x86_64-unknown-linux-musl/release/tehas-mcp)" "units tehas-mcp.service"
t_eq "$(tehas_steps_for libexec/tehas-mcp)" "units tehas-mcp.service"
t_eq "$(tehas_steps_for systemd/tehas-top.service libexec/tehas-mcp | grep -c '^units ')" 1
t_eq "$(tehas_steps_for libexec/dispatch)" ""

t_case "a change to one server asks for that server only"
t_eq "$(tehas_steps_for edge/docs.Caddyfile)" "docs --force-recreate"
t_eq "$(tehas_steps_for edge/router.compose.yaml)" "edge --force-recreate"
# The retired files of the Protolab's server call for the same step, once.
t_eq "$(tehas_steps_for edge/protolab.Caddyfile edge/protolab.compose.yaml)" "edge --force-recreate"

# Caddy reads its config when the process starts, and a changed file is the only
# thing these two steps are ever asked for: docker compose leaves a running
# container with an unchanged service definition exactly as it was. Both servers
# mount their Caddyfile from the host, so both need the container recreated.
for file in docs router; do
  t_case "the $file container is renewed by recreating it"
  t_contains "$(tehas_steps_for edge/$file.Caddyfile)" "--force-recreate"
  t_contains "$(cat "$T_ROOT/host/edge/$file.compose.yaml")" "./$file.Caddyfile:"
done

t_case "the versions the sandbox image pins ask for the build"
t_eq "$(tehas_steps_for versions.env)" "sandbox"
# The sampler of the layout before bin/tehas: retiring it asks for the restart.
t_contains "$(tehas_steps_for bin/tehas-energy)" "units tehas-dispatch.timer tehas-energy.service"
t_contains "$(tehas_steps_for libexec/energy)" "units tehas-dispatch.timer tehas-energy.service"
t_contains "$(tehas_steps_for libexec/top-publish)" "tehas-top.service"

t_case "a new env.sh restarts the running services, which keep the environment of their start"
t_contains "$(tehas_steps_for env.sh)" "units tehas-dispatch.timer tehas-energy.service"

t_case "a change that needs no follow-up asks for none"
t_eq "$(tehas_steps_for libexec/dispatch docs/host.md lib/config.sh)" ""

# The table of the labels is the one thing whose own change owes the labels: a
# release that adds one ends with it on GitHub, not owed in the record. A new
# console owes none, nor does the old script's removal.
t_case "the table of labels asks for the labels step, last"
t_eq "$(tehas_steps_for labels.tsv)" "labels"
t_eq "$(tehas_steps_for labels.tsv libexec/dispatch)" "labels"
t_eq "$(tehas_steps_for labels.tsv-test old-labels.tsv lib/labels.sh libexec/console libexec/labels)" ""
t_eq "$(tehas_steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env \
  edge/docs.compose.yaml edge/router.compose.yaml labels.tsv)" \
  "units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer
sandbox
docs --force-recreate
edge --force-recreate
labels"

t_case "tehas_steps_into gives the same steps in tehas_steps, and empties it first"
for srcs in "systemd/tehas-dispatch.timer sandbox/Dockerfile edge/docs.Caddyfile edge/router.compose.yaml" \
  "libexec/tehas-mcp" "versions.env" "libexec/dispatch"; do
  # shellcheck disable=SC2086  # one source path per word
  tehas_steps_into $srcs
  # shellcheck disable=SC2086
  t_eq "$(if [ "${#tehas_steps[@]}" != 0 ]; then printf '%s\n' "${tehas_steps[@]}"; fi)" "$(tehas_steps_for $srcs)"
done
t_eq "${#tehas_steps[@]}" 0

t_case "every step it asks for is one it defines, and the console takes it as valid"
ids=$(tehas_step_ids | tr '\n' ' ')
while IFS= read -r step; do
  id=${step%% *}
  case " $ids " in
    *" $id "*) ;;
    *) t_bad "tehas_steps_for asked for an id this file does not define: $step" ;;
  esac
  "$T_CONSOLE" update steps valid "$step" || t_bad "a step this file asked for is not one the console accepts: $step"
done < <(tehas_steps_for systemd/tehas-dispatch.service sandbox/build.sh versions.env \
  edge/docs.compose.yaml edge/router.Caddyfile)

# While both tables exist: what install.sh asks for and what the console runs.
t_case "the console names the steps and the units this file names"
t_eq "$("$T_CONSOLE" update steps ids)" "$(tehas_step_ids)"
t_eq "$("$T_CONSOLE" update steps units)" "$(tehas_units)"

t_case "a step recorded under the Protolab's old id is still a step"
t_eq "$(tehas_sort_steps "protolab --force-recreate" "units tehas-dispatch.timer")" \
  "units tehas-dispatch.timer
protolab --force-recreate"

# The alias is said once. Everything that reads a step line asks this file what
# id a token means, so it cannot drift into a place that knows one spelling and
# not the other.
t_case "one function says that a protolab line means the edge step"
t_run '. "$T_ROOT/lib/update-steps.sh"; tehas_step_id protolab; tehas_step_id edge; tehas_step_id docs; tehas_step_id a-step-newer-than-this'
t_eq "$t_out" "edge
edge
docs
a-step-newer-than-this"
t_eq "$(tehas_sort_steps "protolab --force-recreate" "sandbox" "docs --force-recreate")" \
  "sandbox
docs --force-recreate
protolab --force-recreate"
t_eq "$(grep -c -E '^[[:space:]]*protolab\)' "$T_ROOT/lib/update-steps.sh")" 1
t_eq "$(grep -c -F 'edge | protolab' "$T_ROOT/libexec/update")" 0

t_case "the steps run in the order this file lists them, whoever wrote the lines"
t_eq "$(tehas_sort_steps "edge --force-recreate" "docs --force-recreate" \
  "units tehas-dispatch.timer" "sandbox")" \
  "units tehas-dispatch.timer
sandbox
docs --force-recreate
edge --force-recreate"
# A line no step knows is not run, but it keeps its place at the end so it stays
# visible in the record rather than disappearing with the rest of the list.
t_eq "$(tehas_sort_steps "a-step-newer-than-this" "docs --force-recreate")" \
  "docs --force-recreate
a-step-newer-than-this"

t_case "the units step names the preview timer"
t_contains "$(tehas_units)" "tehas-preview.timer"

t_case "the units step names the preview sweep timer"
t_contains "$(tehas_units)" "tehas-preview-sweep.timer"

t_case "the units step names units install.sh actually installs"
missing=""
for unit in $(tehas_units); do
  [ -f "$T_ROOT/host/systemd/$unit" ] || missing="$missing $unit"
done
t_eq "$missing" ""

# The other direction: a unit added to install.sh's file table has to be covered
# by the units step too. A oneshot service is not restarted itself - that would
# run its job off schedule - but its timer is, and the reload reaches it. A
# template (tehas-cycle@.service) has no instance of its own to restart, and
# restarting a running cycle would end it: the reload is all it needs.
t_case "a unit file install.sh installs cannot be left out of the units step"
units_installed=$(tehas_units | tr '\n' ' ')
missing=""
found=0
while IFS= read -r entry; do
  found=$((found + 1))
  unit=${entry#systemd/}
  case "$unit" in *@.*) continue ;; esac
  case " $units_installed " in
    *" $unit "*) continue ;;
  esac
  stem=${unit%.*}
  covered=0
  for named in $units_installed; do
    if [ "${named%.*}" = "$stem" ]; then covered=1; fi
  done
  if [ "$covered" = 0 ]; then missing="$missing $unit"; fi
done < <(sed -n 's#^  "systemd/\([^"|]*\).*#\1#p' "$T_ROOT/install.sh")
# The gate itself has to see the table: an empty list would pass it vacuously.
t_eq "$found" "$(find "$T_ROOT/host/systemd" -type f | wc -l)"
t_eq "$missing" ""

t_case "the unfinished steps of a run have one named place"
T_ENV=("TEHAS_HOME=$T/home/tehas")
t_run '. "$T_ROOT/lib/update-steps.sh"; tehas_pending_file'
t_eq "$t_rc" 0
t_eq "$t_out" "$T/home/tehas/state/update-pending"

t_case "naming the pending file changes nothing"
T_ENV=("TEHAS_HOME=$T/home/nowhere")
t_run '. "$T_ROOT/lib/update-steps.sh"; tehas_pending_file >/dev/null'
t_eq "$t_rc" 0
t_eq "$([ -e "$T/home/nowhere" ] && echo exists || echo none)" "none"

t_case "recording steps writes the record, once each, in the order they run"
T_ENV=("TEHAS_HOME=$T/home/record")
t_run '. "$T_ROOT/lib/update-steps.sh"
tehas_record_steps "docs --force-recreate" "units tehas-dispatch.timer"
tehas_record_steps "sandbox" "docs --force-recreate"
tehas_read_steps'
t_eq "$t_rc" 0
t_eq "$t_out" "units tehas-dispatch.timer
sandbox
docs --force-recreate"

t_case "a record that already holds the steps is left as it is, one that does not is rewritten"
T_ENV=("TEHAS_HOME=$T/home/record-same")
t_run '. "$T_ROOT/lib/update-steps.sh"
f=$(tehas_pending_file)
tehas_record_steps "units tehas-dispatch.timer" sandbox
a=$(stat -c %i "$f")
tehas_record_steps sandbox "units tehas-dispatch.timer"
b=$(stat -c %i "$f")
tehas_record_steps "docs --force-recreate"
c=$(stat -c %i "$f")
printf "sandbox\nunits tehas-dispatch.timer\n" >"$f"
d=$(stat -c %i "$f")
tehas_record_steps sandbox
e=$(stat -c %i "$f")
[ "$a" = "$b" ] && echo kept; [ "$b" != "$c" ] && echo added; [ "$d" != "$e" ] && echo sorted
tehas_read_steps'
t_eq "$t_rc" 0
t_eq "$t_out" "kept
added
sorted
units tehas-dispatch.timer
sandbox"

t_case "a units step another one covers is not recorded twice"
T_ENV=("TEHAS_HOME=$T/home/record-units")
t_run '. "$T_ROOT/lib/update-steps.sh"
tehas_record_steps "units tehas-mcp.service"
tehas_record_steps "units tehas-dispatch.timer tehas-mcp.service" sandbox
tehas_record_steps "units tehas-mcp.service"
tehas_read_steps'
t_eq "$t_rc" 0
t_eq "$t_out" "units tehas-dispatch.timer tehas-mcp.service
sandbox"
T_ENV=("TEHAS_HOME=$T/home/record-units-2")
t_run '. "$T_ROOT/lib/update-steps.sh"
tehas_record_steps "units tehas-mcp.service" "units tehas-top.service"
tehas_read_steps'
t_eq "$t_out" "units tehas-mcp.service
units tehas-top.service"

t_case "a recorded step no version here knows stays recorded, at the end"
T_ENV=("TEHAS_HOME=$T/home/record2")
t_run '. "$T_ROOT/lib/update-steps.sh"
tehas_record_steps "renew-dashboard --now"
tehas_record_steps sandbox
tehas_read_steps'
t_eq "$t_rc" 0
t_eq "$t_out" "sandbox
renew-dashboard --now"

t_case "recording a step reaches the record through install.sh's own path"
# The path is derived from TEHAS_HOME, and the state directory need not exist yet:
# install.sh records before it makes the directories it installs into.
T_ENV=("TEHAS_HOME=$T/home/record3")
t_run '. "$T_ROOT/lib/update-steps.sh"; tehas_record_steps sandbox'
t_eq "$t_rc" 0
t_eq "$(cat "$T/home/record3/state/update-pending")" "sandbox"

# A record this account cannot open holds steps somebody owed. Answering "no lines"
# for it is how an update would come to write back only what it knew, and the steps
# in it would be gone - so a failed read is a failure here, not an empty answer.
t_case "a record that cannot be read is a failure, not an empty answer"
T_ENV=("TEHAS_HOME=$T/home/record4")
t_run '. "$T_ROOT/lib/update-steps.sh"
f=$(tehas_pending_file)
mkdir -p "$(dirname "$f")"
printf "sandbox\n" >"$f"
chmod 000 "$f"
if tehas_read_steps; then echo read-as-empty; else echo refused; fi'
t_eq "$t_rc" 0
t_contains "$t_out" "cannot read the follow-up record"
t_contains "$t_out" "refused"
t_lacks "$t_out" "sandbox"

t_case "an unreadable record is refused even when it looks empty"
T_ENV=("TEHAS_HOME=$T/home/record5")
t_run '. "$T_ROOT/lib/update-steps.sh"
f=$(tehas_pending_file)
mkdir -p "$(dirname "$f")"
: >"$f"
chmod 000 "$f"
if tehas_read_steps; then echo read-as-empty; else echo refused; fi'
t_eq "$t_rc" 0
t_contains "$t_out" "refused"

t_case "merging into a record it cannot read stops instead of replacing it"
T_ENV=("TEHAS_HOME=$T/home/record6")
t_run '. "$T_ROOT/lib/update-steps.sh"
f=$(tehas_pending_file)
mkdir -p "$(dirname "$f")"
printf "units tehas-dispatch.timer\n" >"$f"
chmod 000 "$f"
if tehas_record_steps sandbox; then echo replaced; else echo refused; fi'
t_eq "$t_rc" 0
t_contains "$t_out" "refused"
# What was owed is still there, unreadable as it was.
chmod 644 "$T/home/record6/state/update-pending"
t_eq "$(cat "$T/home/record6/state/update-pending")" "units tehas-dispatch.timer"

T_ENV=("TEHAS_HOME=$T/home/record7")
t_case "the record is replaced whole, and leaves no temporary file behind"
# Each rewrite goes through a temporary file moved over the record: a run that died
# mid-write would leave the steps already owed there rather than half of a new
# list, and a finished one leaves nothing but the record.
t_run '. "$T_ROOT/lib/update-steps.sh"
tehas_write_steps sandbox "docs --force-recreate"
tehas_record_steps "units tehas-dispatch.timer"'
t_eq "$t_rc" 0
t_eq "$(ls -A "$T/home/record7/state")" "update-pending"
t_eq "$(cat "$T/home/record7/state/update-pending")" "units tehas-dispatch.timer
sandbox
docs --force-recreate"
# No lines means no record.
t_run '. "$T_ROOT/lib/update-steps.sh"; tehas_write_steps'
t_eq "$t_rc" 0
t_eq "$(ls -A "$T/home/record7/state")" ""
T_ENV=()

t_done
