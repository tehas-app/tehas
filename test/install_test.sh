#!/usr/bin/env bash
# Tests for install.sh: what it installs, that an invalid instance changes
# nothing, and that drift is reported and repaired. Docker and systemctl are
# stubs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"
# The one place the follow-ups are named: install.sh prints what it defines here,
# and tehas-update runs the same ids, so a case can pin both sides to it.
# shellcheck source=lib/update-steps.sh
. "$T_ROOT/lib/update-steps.sh"

install="$T_ROOT/install.sh"
home=$T/home
th=$home/tehas
T_ENV=(T_IN_TEST=1)

t_stub docker
t_stub systemctl

# An instance directory with settings and a model list that matches them.
instance() {
  t_instance "${T_MINIMAL[@]}" "$@"
  cat >"$TEHAS_INSTANCE_DIR/models.json" <<'EOF'
{"providers": {"test-provider": {"models": [{"id": "test-model"}, {"id": "session-model"}]}}}
EOF
}
# Everything install.sh put on the host, relative to the scratch home.
installed_files() { (cd "$home" && find . -type f ! -name '*.prev' | LC_ALL=C sort); }
fresh() {
  rm -rf "$home" "$T"/calls/*
  mkdir -p "$home"
}

files="./.config/systemd/user/tehas-dispatch.service
./.config/systemd/user/tehas-dispatch.timer
./.config/systemd/user/tehas-energy.service
./.config/systemd/user/tehas-protolab.service
./.config/systemd/user/tehas-protolab.timer
./.pi/agent/mcp.json
./.pi/agent/models.json
./.pi/agent/settings.json
./tehas/bin/tehas-console.ts
./tehas/bin/tehas-dispatch
./tehas/bin/tehas-energy
./tehas/bin/tehas-health
./tehas/bin/tehas-protolab
./tehas/bin/tehas-status
./tehas/bin/tehas-test
./tehas/bin/tehas-update
./tehas/bin/tehas-work
./tehas/edge/docs.Caddyfile
./tehas/edge/docs.compose.yaml
./tehas/edge/protolab.Caddyfile
./tehas/edge/protolab.compose.yaml
./tehas/env.sh
./tehas/lib/config.sh
./tehas/lib/update-steps.sh
./tehas/sandbox/Dockerfile
./tehas/sandbox/build.sh
./tehas/versions.env"
# The manual and the front page are whole directories; leave them out of the
# exact comparison and check them separately. So is state/, which is what the host
# keeps for itself - the record of follow-ups owed among it - not a file install.sh
# puts.
core_files() { installed_files | grep -v -e '^./tehas/docs/' -e '^./tehas/fun/' -e '^./tehas/state/'; }
sorted() { printf '%s\n' "$@" | LC_ALL=C sort; }

# The follow-ups a fresh install of this repository asks for, taken from the one
# place that names them and the sources a fresh host has none of.
next_steps() {
  tehas_steps_for systemd/tehas-dispatch.service systemd/tehas-dispatch.timer \
    systemd/tehas-energy.service systemd/tehas-protolab.service \
    systemd/tehas-protolab.timer sandbox/Dockerfile sandbox/build.sh versions.env \
    edge/docs.Caddyfile edge/docs.compose.yaml edge/protolab.Caddyfile \
    edge/protolab.compose.yaml
}

t_case "a fresh install: exactly these files"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(core_files)" "$(sorted "$files")"
# The follow-ups these copies call for are asked for by id and recorded as owed.
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^next: //p')" "$(next_steps)"
t_eq "$(cat "$th/state/update-pending")" "$(next_steps)"
# install.sh asks systemd for nothing: a copied unit file is not in effect until
# the step that restarts the units has reloaded it.
t_eq "$(t_calls systemctl)" ""
t_eq "$(t_calls docker)" ""

t_case "the manual and the front page are installed"
t_eq "$(cd "$th/docs" && find . -type f | wc -l)" "$(find "$T_ROOT/host/docs" -type f | wc -l)"
t_eq "$(cd "$th/fun" && find . -type f | wc -l)" "$(find "$T_ROOT/host/fun" -type f | wc -l)"

t_case "the instance's model list is what Pi gets"
t_eq "$(cat "$home/.pi/agent/models.json")" "$(cat "$TEHAS_INSTANCE_DIR/models.json")"

t_case "nothing installed names a file that is absent"
missing=""
for unit in "$home"/.config/systemd/user/*; do
  while read -r ref; do
    case "$ref" in */logs/*) ref=$(dirname "$ref") ;; esac
    [ -e "$ref" ] || missing="$missing $ref"
  done < <(grep -o '%h/tehas/[A-Za-z0-9_./-]*' "$unit" | sed "s|%h|$home|")
done
for file in docs.compose.yaml protolab.compose.yaml; do
  for ref in $(grep -o '"\.\.\?/[A-Za-z0-9_./-]*:' "$th/edge/$file" | tr -d '":'); do
    [ -e "$th/edge/$ref" ] || missing="$missing $ref"
  done
done
t_eq "$missing" ""
t_case "a second run changes nothing"
t_run "$install"
t_contains "$t_out" "tehas is up to date"

t_case "--check on a matching install reports four lines and exits 0"
# The follow-ups of the install above are taken as done, which is what running
# tehas-update does to the record. A host that still owes one is another case.
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Instance:     $T/instance OK"
t_contains "$t_out" "Dependencies: not checked (running inside the tests)"
t_contains "$t_out" "Installed:    OK"
t_contains "$t_out" "Tests:        not checked (running inside the tests)"

t_case "--check reports drift, changes nothing and exits 1"
echo "# edited on the host" >>"$th/bin/tehas-dispatch"
before=$(installed_files | xargs -I{} sh -c "cd '$home' && cksum '{}'")
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Installed:    DRIFT (1 files)"
t_contains "$t_out" "differs: $th/bin/tehas-dispatch"
t_eq "$(installed_files | xargs -I{} sh -c "cd '$home' && cksum '{}'")" "$before"

t_case "a real run repairs drift and keeps the old file as .prev"
t_run "$install"
t_contains "$t_out" "installed: $th/bin/tehas-dispatch"
t_eq "$(cmp -s "$th/bin/tehas-dispatch" "$T_ROOT/bin/tehas-dispatch" && echo same)" "same"
t_contains "$(cat "$th/bin/tehas-dispatch.prev")" "# edited on the host"

t_case "an instance with a missing setting installs nothing and exits 78"
fresh
t_instance TEHAS_CONTROL_ISSUE=1
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "tehas: TEHAS_REPO is not set"
t_contains "$t_out" "nothing was installed"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "no instance at all installs nothing and exits 78"
rm -rf "$T/instance"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "no instance.env in"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "a model missing from models.json is a finding"
fresh
instance PI_ENSEMBLE_SUBAGENT_MODEL=other-model
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "models.json does not list model other-model under provider test-provider"

t_case "a missing models.json is a finding"
instance
rm "$TEHAS_INSTANCE_DIR/models.json"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "INVALID"
t_contains "$t_out" "no models.json in"

t_case "the product's session model must be listed too"
instance
mkdir -p "$th/repos/widget/.pi"
echo '{"defaultProvider":"test-provider","defaultModel":"missing-model"}' >"$th/repos/widget/.pi/settings.json"
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "does not list the product's session model test-provider/missing-model"
echo '{"defaultProvider":"test-provider","defaultModel":"session-model"}' >"$th/repos/widget/.pi/settings.json"
t_run "$install"
t_eq "$t_rc" 0

for asked in TEHAS_PREVIEWS=1 TEHAS_CLONES=1; do
  t_case "an instance that asks for $asked installs nothing and exits 78"
  fresh
  instance "$asked"
  t_run "$install"
  t_eq "$t_rc" 78
  t_contains "$t_out" "${asked%%=*} is set and this version of tehas has no previews or database clones"
  t_eq "$(find "$home" -type f | wc -l)" 0
done

t_case "settings anyone can write are a finding"
instance
chmod o+w "$TEHAS_INSTANCE_DIR/instance.env"
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "is writable by everyone"
chmod o-w "$TEHAS_INSTANCE_DIR/instance.env"

t_case "settings reached through a symlink are judged by the file they point at"
fresh
instance
mv "$TEHAS_INSTANCE_DIR/instance.env" "$T/real-instance.env"
chmod 600 "$T/real-instance.env"
ln -s "$T/real-instance.env" "$TEHAS_INSTANCE_DIR/instance.env"
t_run "$install --check"
t_lacks "$t_out" "INVALID"
t_contains "$t_out" "Instance:     $T/instance OK"
rm "$TEHAS_INSTANCE_DIR/instance.env"

t_case "an instance's own Pi settings are used when it has them"
fresh
instance
echo '{"defaultProvider":"test-provider"}' >"$TEHAS_INSTANCE_DIR/settings.json"
t_run "$install"
t_eq "$(cat "$home/.pi/agent/settings.json")" '{"defaultProvider":"test-provider"}'

for name in example tehas-tehas; do
  t_case "the shipped instance '$name' is valid"
  fresh
  export TEHAS_INSTANCE_DIR=$T_ROOT/instances/$name
  t_run "$install --check"
  t_lacks "$t_out" "INVALID"
  t_contains "$t_out" "Instance:     $T_ROOT/instances/$name OK"
done

t_case "a changed unit file asks for the units step, whatever systemd answers"
fresh
instance
# install.sh asks systemd for nothing at all, so nothing here can report a changed
# unit as applied while systemd still holds the old definition. The reload belongs
# to the step that restarts the units, which is the only place that can tell
# whether it happened.
t_stub systemctl 'echo "Failed to connect to bus: No such file or directory" >&2; exit 1'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: units tehas-dispatch.timer tehas-energy.service tehas-protolab.timer"
t_eq "$(t_calls systemctl)" ""
t_contains "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer"

t_case "a run that dies partway leaves the copies it made owed"
fresh
instance
# A checkout missing one source: put() stops the run with no follow-up report at
# all. The files it did copy are on the host, and what they call for was recorded
# as each of them went - so the next tehas-update still runs those steps, and
# install.sh --check still says the host is not current.
part=$T/partial
rm -rf "$part"
mkdir -p "$part"
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$part/"
chmod +x "$part/install.sh"
rm "$part/host/pi/mcp.json"
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "is missing from this checkout"
t_lacks "$t_out" "next: "
t_eq "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer tehas-energy.service tehas-protolab.timer
sandbox
docs --force-recreate
protolab --force-recreate"

t_case "the follow-ups it prints are the ids lib/update-steps.sh defines"
fresh
instance
t_stub systemctl
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^next: //p')" \
  "$(tehas_steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env edge/docs.compose.yaml edge/protolab.compose.yaml)"
# Steps, not shell text for a person to paste.
t_lacks "$t_out" "docker compose -f"
t_lacks "$t_out" "systemctl --user enable"

t_case "--check reports follow-ups an earlier run left undone"
# One of the two lines is a step no version of tehas here knows: install.sh --check
# reports whatever is in the record, because it cannot perform any of them either.
fresh
instance
t_run "$install"
mkdir -p "$th/state"
printf 'sandbox\ndocs\n' >"$(TEHAS_HOME=$th tehas_pending_file)"
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

t_case "an install will not record into a record it cannot read"
fresh
instance
mkdir -p "$th/state"
printf 'sandbox\n' >"$th/state/update-pending"
chmod 000 "$th/state/update-pending"
t_run "$install"
t_eq "$t_rc" 1
t_contains "$t_out" "cannot read the follow-up record"
# It stopped rather than replace what was owed with the steps it was copying.
chmod 644 "$th/state/update-pending"
t_eq "$(cat "$th/state/update-pending")" "sandbox"

t_done
