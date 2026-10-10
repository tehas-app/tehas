#!/usr/bin/env bash
# Tests for install.sh: what it installs, that an invalid instance changes
# nothing, and that drift is reported and repaired. Docker and systemctl are
# stubs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

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
./tehas/bin/tehas-index
./tehas/bin/tehas-protolab
./tehas/bin/tehas-status
./tehas/bin/tehas-work
./tehas/edge/docs.Caddyfile
./tehas/edge/docs.compose.yaml
./tehas/edge/protolab.Caddyfile
./tehas/edge/protolab.compose.yaml
./tehas/env.sh
./tehas/lib/config.sh
./tehas/sandbox/Dockerfile
./tehas/sandbox/build.sh
./tehas/versions.env"
# The manual and the front page are whole directories; leave them out of the
# exact comparison and check them separately.
core_files() { installed_files | grep -v -e '^./tehas/docs/' -e '^./tehas/fun/'; }
sorted() { printf '%s\n' "$@" | LC_ALL=C sort; }

t_case "a fresh install: exactly these files"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(core_files)" "$(sorted "$files")"

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

# The code index: built by the first real run that finds both the sandbox
# image and the product checkout, and never by --check.
product() {
  mkdir -p "$T/git/acme"
  [ -d "$T/git/acme/widget" ] || {
    git init -q -b main "$T/git/acme/widget"
    git -C "$T/git/acme/widget" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
  }
  git clone -q "$T/git/acme/widget" "$th/repos/widget"
}
index_runs() { t_calls docker | grep -c -e '^run .*index_repository' || true; }
t_stub docker 'case "$1" in
  image) exit "${IMAGE_RC:-0}" ;;
  run) [ "${RUN_RC:-0}" = 0 ] || exit "$RUN_RC"; echo "{\"status\":\"indexed\",\"nodes\":1,\"edges\":2}" ;;
esac'

t_case "an install without the product checkout names the indexing step"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: $th/bin/tehas-index"
t_eq "$(index_runs)" 0

t_case "an install without the sandbox image names the indexing step"
product
T_ENV=(T_IN_TEST=1 IMAGE_RC=1)
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "next: $th/bin/tehas-index"
t_eq "$(index_runs)" 0

t_case "--check builds no index"
T_ENV=(T_IN_TEST=1)
t_run "$install --check"
t_eq "$(index_runs)" 0
t_eq "$(find "$th/state/index.json" 2>/dev/null | wc -l)" 0

t_case "a failed indexing is a warning, and the install still succeeds"
T_ENV=(T_IN_TEST=1 RUN_RC=1)
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the code index was not built"
t_contains "$t_out" "next: $th/bin/tehas-index"

t_case "with the image and the checkout, the install builds the index"
rm -f "$T"/calls/*
T_ENV=(T_IN_TEST=1)
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas-index: widget indexed"
t_lacks "$t_out" "next: $th/bin/tehas-index"
t_eq "$(index_runs)" 1
t_eq "$(jq -r .nodes "$th/state/index.json")" 1

t_case "a later install leaves the index alone"
t_run "$install"
t_contains "$t_out" "tehas is up to date"
t_eq "$(index_runs)" 1
t_stub docker

for name in example tehas-tehas; do
  t_case "the shipped instance '$name' is valid"
  fresh
  export TEHAS_INSTANCE_DIR=$T_ROOT/instances/$name
  t_run "$install --check"
  t_lacks "$t_out" "INVALID"
  t_contains "$t_out" "Instance:     $T_ROOT/instances/$name OK"
done

t_done
