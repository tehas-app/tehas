#!/usr/bin/env bash
# Tests for install.sh: the shipped instances are valid, and the follow-ups a
# run asks for are recorded and reported. install_steps_machine_test.sh holds
# the rest, install_steps_store_test.sh the real store's cases.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

t_stub nproc 'echo 12'
for name in example tehas-tehas; do
  t_case "the shipped instance '$name' is valid"
  fresh
  # A copy with the renames of api/renames-1.0.tsv: a running factory's instance.
  rm -rf "$T/shipped" && cp -R "$T_ROOT/instances/$name" "$T/shipped"
  while IFS=$'\t' read -r kind old new _; do
    [ "$kind" = setting ] && [ -n "$new" ] || continue
    sed -i "s/^$old=/$new=/" "$T/shipped/instance.env"
  done <"$T_ROOT/api/renames-1.0.tsv"
  export TEHAS_INSTANCE_DIR=$T/shipped
  # The tokens are the host's, not the instance's: stand-ins for each it names.
  keep=("${T_ENV[@]}")
  while read -r k; do
    [ -z "$k" ] || T_ENV+=("$k=stand-in")
  done < <(jq -r '.providers[] | (.apiKey, .backupApiKey) | strings
    | select(startswith("$")) | ltrimstr("$") | ltrimstr("{") | rtrimstr("}")' "$TEHAS_INSTANCE_DIR/models.json")
  t_run "$install --check"
  t_lacks "$t_out" "INVALID"
  t_contains "$t_out" "Instance:     $T/shipped OK"
  T_ENV=("${keep[@]}")
done
rm -f "$T/bin/nproc"

t_case "a changed unit file asks for the units step, whatever systemd answers"
fresh
instance
# Past a first install's enable, install.sh asks systemd for nothing: the reload
# belongs to the step that restarts the units. Here the first install's enable
# fails, so the units stay owed, and so does the first install.
t_stub systemctl 'echo "Failed to connect to bus: No such file or directory" >&2; exit 1'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: units tehas-dispatch.timer tehas-top.service tehas-mcp.service tehas-mcp-fetch.socket tehas-preview.timer tehas-preview-sweep.timer tehas-clean.timer"
t_eq "$(t_calls systemctl)" "--user daemon-reload"
t_contains "$t_out" "the first install's labels or units are not done: systemctl --user enable failed"
t_eq "$([ -e "$th/state/first-install" ] && echo owed)" owed
t_contains "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer"

t_case "a run that dies partway leaves the copies it made owed"
fresh
instance
# A checkout missing one source: put() stops the run with no report, but what the
# files it did copy call for was recorded, for tehas update and --check.
part=$T/partial
rm -rf "$part"
mkdir -p "$part"
cp -R "$fixture/install.sh" "$fixture/lib" "$fixture/host" "$fixture/bin" \
  "$fixture/libexec" "$part/"
t_console_source "$part" "$fixture"
chmod +x "$part/install.sh"
rm "$part/host/pi/mcp.json"
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "is missing from this checkout"
t_lacks "$t_out" "next: "
t_eq "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer tehas-top.service tehas-mcp.service tehas-mcp-fetch.socket tehas-preview.timer tehas-preview-sweep.timer tehas-clean.timer
sandbox
docs --force-recreate
edge --force-recreate"

t_case "the follow-ups it prints are the ids the console defines"
fresh
instance
t_stub systemctl
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^next: //p')" \
  "$("$T_CONSOLE" update steps for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env edge/docs.compose.yaml edge/router.compose.yaml labels.tsv)"
# Steps, not shell text for a person to paste.
t_lacks "$t_out" "docker compose -f"
t_lacks "$t_out" "systemctl --user enable"

t_case "a later install resets the failed tehas units and starts a failed path unit again"
fresh
instance
t_stub systemctl
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$(t_calls systemctl)" "reset-failed"
rm -f "$T/calls/systemctl"
t_stub systemctl 'case "$*" in "--user is-failed tehas-dispatch.path") echo failed ;; esac'
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl)" "--user is-failed tehas-dispatch.path
--user reset-failed tehas-*
--user start tehas-dispatch.path"
t_contains "$t_out" "systemd: tehas-dispatch.path had failed; reset and started"

t_case "a later install with no failed path unit resets the others and starts nothing"
rm -f "$T/calls/systemctl"
t_stub systemctl 'case "$*" in "--user is-failed tehas-dispatch.path") echo active; exit 1 ;; esac'
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls systemctl)" "--user is-failed tehas-dispatch.path
--user reset-failed tehas-*"
t_stub systemctl

t_case "--check reports follow-ups an earlier run left undone"
# One line is a step no tehas knows: --check reports whatever the record holds.
fresh
instance
t_run "$install"
mkdir -p "$th/state"
printf 'sandbox\ndocs\n' >"$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Follow-ups:   NOT DONE (2 from an earlier run"
t_contains "$t_out" "Installed:    OK"
fresh
instance
t_run "$install --check"
t_lacks "$t_out" "Follow-ups:"

t_case "--check says a record it cannot read is not done, and does not count it"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
mkdir -p "$th/state"
printf 'sandbox\ndocs --force-recreate\n' >"$th/state/update-pending"
chmod 000 "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Follow-ups:   NOT DONE (the record $th/state/update-pending cannot be read)"
chmod 644 "$th/state/update-pending"
t_eq "$(cat "$th/state/update-pending")" "sandbox
docs --force-recreate"

t_done
