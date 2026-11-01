#!/usr/bin/env bash
# Tests for the follow-up steps: what install.sh and the console's own table must
# agree on, the sources that call for each step through the verbs install.sh
# calls, and the reader of the record that stays in lib/update-steps.sh. What
# counts as a step line, the order the steps run in and how the record is merged
# are the Rust tests of console/src/update/steps.rs.
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=lib/update-steps.sh
. "$T_ROOT/lib/update-steps.sh"
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
steps_for() { "$T_CONSOLE" update steps for "$@"; }
units() { "$T_CONSOLE" update steps units; }

t_case "the changed sources ask for the steps, systemd first"
t_eq "$(steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env \
  edge/docs.compose.yaml edge/router.compose.yaml libexec/dispatch)" \
  "units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer
sandbox
docs --force-recreate
edge --force-recreate"

t_case "a new MCP server restarts its unit, and only it"
t_eq "$(steps_for /src/console/target/x86_64-unknown-linux-musl/release/tehas-mcp)" "units tehas-mcp.service"
t_eq "$(steps_for libexec/tehas-mcp)" "units tehas-mcp.service"
t_eq "$(steps_for systemd/tehas-top.service libexec/tehas-mcp | grep -c '^units ')" 1
t_eq "$(steps_for libexec/dispatch)" ""

t_case "a change to one server asks for that server only"
t_eq "$(steps_for edge/docs.Caddyfile)" "docs --force-recreate"
t_eq "$(steps_for edge/router.compose.yaml)" "edge --force-recreate"
# The retired files of the Protolab's server call for the same step, once.
t_eq "$(steps_for edge/protolab.Caddyfile edge/protolab.compose.yaml)" "edge --force-recreate"

# Caddy reads its config when the process starts, and a changed file is the only
# thing these two steps are ever asked for: docker compose leaves a running
# container with an unchanged service definition exactly as it was. Both servers
# mount their Caddyfile from the host, so both need the container recreated.
for file in docs router; do
  t_case "the $file container is renewed by recreating it"
  t_contains "$(steps_for edge/$file.Caddyfile)" "--force-recreate"
  t_contains "$(cat "$T_ROOT/host/edge/$file.compose.yaml")" "./$file.Caddyfile:"
done

t_case "the versions the sandbox image pins ask for the build"
t_eq "$(steps_for versions.env)" "sandbox"
# The sampler of the layout before bin/tehas: retiring it asks for the restart.
t_contains "$(steps_for bin/tehas-energy)" "units tehas-dispatch.timer tehas-energy.service"
t_contains "$(steps_for libexec/energy)" "units tehas-dispatch.timer tehas-energy.service"
t_contains "$(steps_for libexec/top-publish)" "tehas-top.service"

t_case "a new env.sh restarts the running services, which keep the environment of their start"
t_contains "$(steps_for env.sh)" "units tehas-dispatch.timer tehas-energy.service"

t_case "a change that needs no follow-up asks for none"
t_eq "$(steps_for libexec/dispatch docs/host.md lib/config.sh)" ""

# The table of the labels is the one thing whose own change owes the labels: a
# release that adds one ends with it on GitHub, not owed in the record. A new
# console owes none, nor does the old script's removal.
t_case "the table of labels asks for the labels step, last"
t_eq "$(steps_for labels.tsv)" "labels"
t_eq "$(steps_for labels.tsv libexec/dispatch)" "labels"
t_eq "$(steps_for labels.tsv-test old-labels.tsv lib/labels.sh libexec/console libexec/labels)" ""
t_eq "$(steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env \
  edge/docs.compose.yaml edge/router.compose.yaml labels.tsv)" \
  "units tehas-dispatch.timer tehas-energy.service tehas-top.service tehas-mcp.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer tehas-refresh.timer tehas-review.timer tehas-clean.timer
sandbox
docs --force-recreate
edge --force-recreate
labels"

t_case "the units step names the preview timer"
t_contains "$(units)" "tehas-preview.timer"

t_case "the units step names the preview sweep timer"
t_contains "$(units)" "tehas-preview-sweep.timer"

t_case "the units step names units install.sh actually installs"
missing=""
for unit in $(units); do
  [ -f "$T_ROOT/host/systemd/$unit" ] || missing="$missing $unit"
done
t_eq "$missing" ""

# The other direction: a unit added to install.sh's file table has to be covered
# by the units step too. A oneshot service is not restarted itself - that would
# run its job off schedule - but its timer is, and the reload reaches it. A
# template (tehas-cycle@.service) has no instance of its own to restart, and
# restarting a running cycle would end it: the reload is all it needs.
t_case "a unit file install.sh installs cannot be left out of the units step"
units_installed=$(units | tr '\n' ' ')
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
T_ENV=("TEHAS_HOME=$T/home/record" "TEHAS_CONSOLE=$T_CONSOLE")
t_run '"$TEHAS_CONSOLE" update steps record edge/docs.Caddyfile libexec/tehas-mcp
"$TEHAS_CONSOLE" update steps record sandbox/Dockerfile edge/docs.compose.yaml
. "$T_ROOT/lib/update-steps.sh"; tehas_read_steps'
t_eq "$t_rc" 0
t_eq "$t_out" "units tehas-mcp.service
sandbox
docs --force-recreate"

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

t_case "recording into a record it cannot read stops instead of replacing it"
T_ENV=("TEHAS_HOME=$T/home/record6" "TEHAS_CONSOLE=$T_CONSOLE")
t_run 'f=$TEHAS_HOME/state/update-pending
mkdir -p "$(dirname "$f")"
printf "units tehas-dispatch.timer\n" >"$f"
chmod 000 "$f"
if "$TEHAS_CONSOLE" update steps record sandbox/Dockerfile; then echo replaced; else echo refused; fi'
t_eq "$t_rc" 0
t_contains "$t_out" "refused"
chmod 644 "$T/home/record6/state/update-pending"
t_eq "$(cat "$T/home/record6/state/update-pending")" "units tehas-dispatch.timer"
T_ENV=()

t_done
