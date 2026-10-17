#!/usr/bin/env bash
# Tests for lib/update-steps.sh: the follow-up names install.sh prints and
# tehas update runs, the sources that call for each one, what counts as a step
# line rather than text, the order the steps run in, and the record of steps owed.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"
# shellcheck source=lib/update-steps.sh
. "$T_ROOT/lib/update-steps.sh"

t_case "the changed sources ask for the steps, systemd first"
t_eq "$(tehas_steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env \
  edge/docs.compose.yaml edge/router.compose.yaml libexec/dispatch)" \
  "units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer
sandbox
docs --force-recreate
edge --force-recreate"

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

t_case "a new env.sh restarts the running services, which keep the environment of their start"
t_contains "$(tehas_steps_for env.sh)" "units tehas-dispatch.timer tehas-energy.service"

t_case "a change that needs no follow-up asks for none"
t_eq "$(tehas_steps_for libexec/dispatch docs/host.md lib/config.sh)" ""

t_case "every step it asks for is one it defines, and reads back as valid"
ids=$(tehas_step_ids | tr '\n' ' ')
while IFS= read -r step; do
  id=${step%% *}
  case " $ids " in
    *" $id "*) ;;
    *) t_bad "tehas_steps_for asked for an id this file does not define: $step" ;;
  esac
  if ! tehas_valid_step "$step"; then t_bad "a step this file asked for is not one it accepts: $step"; fi
done < <(tehas_steps_for systemd/tehas-dispatch.service sandbox/build.sh versions.env \
  edge/docs.compose.yaml edge/router.Caddyfile)

t_case "a line that is not a step this file knows is refused"
# A compose id on its own belongs on this list: docker compose up -d without
# --force-recreate leaves a running container serving the config it loaded at its
# start, so a bare id would perform the weaker action and then be counted as the
# step having run.
for bad in "" "rm -rf something" "units" "units --help tehas-dispatch.timer" \
  "units dispatch" "units tehas-dispatch.timers" "units ../etc/passwd" \
  "units gnome-shell.service" "units systemd-journald.service" \
  "docs" "edge" "docs restart" "docs up -d" "docs --always-recreate" \
  "docs --force-recreate up" "docs --force-recreate --force-recreate" \
  "edge --always-recreate" "sandbox --now"; do
  t_case "refused: [$bad]"
  if tehas_valid_step "$bad"; then t_bad "accepted: $bad"; fi
done

t_case "a step line is read as words, not as a command"
# Names are parsed as unit names, and nothing here runs: the file only answers
# questions. Instance names and odd characters parse as the single word they are.
tehas_is_unit "tehas-energy@1.service" || t_bad "a unit name with @ was refused"
tehas_is_unit "socket-2.socket" || t_bad "a unit name with - and a digit was refused"
tehas_is_unit "--force-recreate" && t_bad "an option was taken for a unit name"
# A name that parses is still not enough: the units step may only name units this
# version of tehas installs, so no step line reaches some other unit of the host's.
tehas_valid_step "units $(tehas_units | tr '\n' ' ')" ||
  t_bad "the units install.sh names were refused"
tehas_valid_step "units gnome-shell.service" &&
  t_bad "a unit tehas does not install was accepted"
tehas_valid_step "docs --force-recreate" || t_bad "the flag install.sh asks for was refused"
tehas_valid_step "docs" && t_bad "a bare docs was accepted, which is the weaker renewal"

t_case "a step recorded under the Protolab's old id is still a step"
t_run '. "$T_ROOT/lib/update-steps.sh"; tehas_valid_step "protolab --force-recreate"; echo rc=$?'
t_eq "$t_out" "rc=0"
t_run '. "$T_ROOT/lib/update-steps.sh"; tehas_valid_step "edge --force-recreate"; echo rc=$?'
t_eq "$t_out" "rc=0"
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
# Read through that function, the alias carries the rules of the id it stands
# for: a bare protolab is the weaker renewal, and is refused like a bare edge.
t_run '. "$T_ROOT/lib/update-steps.sh"; tehas_valid_step "protolab"; echo rc=$?'
t_eq "$t_out" "rc=1"
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
