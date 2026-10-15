#!/usr/bin/env bash
# Tests for install.sh: what it installs, that an invalid instance changes
# nothing, that drift is reported and repaired, and that a stale copy of the
# command root installed for the other accounts is reported and left alone.
# Docker, systemctl, sudo and cargo are stubs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"
# The one place the follow-ups are named: install.sh prints what it defines here,
# and tehas update runs the same ids, so a case can pin both sides to it.
# shellcheck source=lib/update-steps.sh
. "$T_ROOT/lib/update-steps.sh"

install="$T_ROOT/install.sh"
home=$T/home
th=$home/tehas
# bootstrap.sh dev writes two files outside any home, and install.sh reads them to
# know whether this host lets other accounts start a session. Pointed at scratch
# in every run here, so no test reads or writes the real /etc or /usr/local/bin.
dev_etc=$T/etc
dev_bin=$T/usr-local-bin
t_env() { T_ENV=(BOOTSTRAP_ETC="$dev_etc" BOOTSTRAP_BIN="$dev_bin" T_IN_TEST=1 "$@"); }
t_env

# The previews' network as install.sh wants to find it, and how it asks.
net_good='case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") echo "true bridge false isolated" ;; esac'
net_inspect='version --format {{.Server.Version}}
network inspect --format {{.Internal}} {{.Driver}} {{.EnableIPv6}} {{index .Options "com.docker.network.bridge.gateway_mode_ipv4"}} tehas-previews'
t_stub docker "$net_good"
t_stub systemctl
t_stub_cargo

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

files="./.config/systemd/user/tehas-cycle@.service
./.config/systemd/user/tehas-dispatch.service
./.config/systemd/user/tehas-dispatch.timer
./.config/systemd/user/tehas-energy.service
./.config/systemd/user/tehas-improve.service
./.config/systemd/user/tehas-improve.timer
./.config/systemd/user/tehas-preview-sweep.service
./.config/systemd/user/tehas-preview-sweep.timer
./.config/systemd/user/tehas-preview.service
./.config/systemd/user/tehas-preview.timer
./.config/systemd/user/tehas-protolab.service
./.config/systemd/user/tehas-protolab.timer
./.pi/agent/mcp.json
./.pi/agent/models.json
./.pi/agent/settings.json
./tehas/bin/tehas
./tehas/bin/tehasbot
./tehas/edge/docs.Caddyfile
./tehas/edge/docs.compose.yaml
./tehas/edge/router.Caddyfile
./tehas/edge/router.compose.yaml
./tehas/env.sh
./tehas/lib/attachments.sh
./tehas/lib/config.sh
./tehas/lib/cycles.sh
./tehas/lib/edge.sh
./tehas/lib/improve-prompt.md
./tehas/lib/improve.sh
./tehas/lib/labels.sh
./tehas/lib/live.sh
./tehas/lib/planning.sh
./tehas/lib/questions.sh
./tehas/lib/sandbox.sh
./tehas/lib/status.sh
./tehas/lib/top.sh
./tehas/lib/slice.sh
./tehas/lib/update-steps.sh
./tehas/libexec/console
./tehas/libexec/console.stamp
./tehas/libexec/cycle
./tehas/libexec/dev
./tehas/libexec/dev-login
./tehas/libexec/dispatch
./tehas/libexec/energy
./tehas/libexec/health
./tehas/libexec/improve
./tehas/libexec/index
./tehas/libexec/labels
./tehas/libexec/plan
./tehas/libexec/preview
./tehas/libexec/protolab
./tehas/libexec/status
./tehas/libexec/summary
./tehas/libexec/top
./tehas/libexec/update
./tehas/libexec/work
./tehas/sandbox/Dockerfile
./tehas/sandbox/build.sh
./tehas/sandbox/npm/playwright/package-lock.json
./tehas/sandbox/npm/playwright/package.json
./tehas/versions.env"
# The manual and the front page are whole directories; leave them out of the
# exact comparison and check them separately. So is state/, which is what the host
# keeps for itself - the record of follow-ups owed among it - not a file install.sh
# puts.
core_files() { installed_files | grep -v -e '^./tehas/docs/' -e '^./tehas/fun/' -e '^./tehas/state/'; }
sorted() { printf '%s\n' "$@" | LC_ALL=C sort; }
# Every file under the scratch home but the update lock, which a real run takes
# before it builds or copies anything.
host_files() { find "$home" -type f ! -path "$th/state/update.lock"; }

# The follow-ups a fresh install of this repository asks for, taken from the one
# place that names them and the sources a fresh host has none of.
next_steps() {
  tehas_steps_for systemd/tehas-cycle@.service systemd/tehas-dispatch.service systemd/tehas-dispatch.timer \
    systemd/tehas-energy.service systemd/tehas-improve.service systemd/tehas-improve.timer \
    systemd/tehas-preview.service \
    systemd/tehas-preview.timer systemd/tehas-preview-sweep.service systemd/tehas-preview-sweep.timer systemd/tehas-protolab.service systemd/tehas-protolab.timer sandbox/Dockerfile sandbox/build.sh \
    sandbox/npm/playwright/package.json sandbox/npm/playwright/package-lock.json versions.env \
    edge/docs.Caddyfile edge/docs.compose.yaml edge/router.Caddyfile \
    edge/router.compose.yaml
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
# It makes the previews' network where there is none, and asks nothing else.
t_eq "$(t_calls docker)" "$net_inspect"
t_lacks "$t_out" "tehas-previews"
t_eq "$([ -d "$th/preview" ] && echo there)" "there"
# A host gets its tools from bootstrap.sh before its first install.
t_lacks "$t_out" "bootstrap.sh user"

t_case "another update holding the lock stops a real run before it installs anything"
fresh
instance
mkdir -p "$th/state"
lock=$th/state/update.lock
t_run "exec 8>>'$lock'; flock -n 8 || exit 9; '$install'"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas: another update is running ($lock); nothing was installed"
t_eq "$(core_files)" ""
t_eq "$(t_calls cargo)" ""

t_case "the lock handed over by the update holding it is taken"
t_run "exec 8>>'$lock'; flock -n 8 || exit 9; TEHAS_UPDATE_LOCK_FD=8 '$install'"
t_eq "$t_rc" 0
t_eq "$(core_files)" "$(sorted "$files")"

t_case "a descriptor on the lock file that does not hold the lock is not taken for it"
fresh
instance
mkdir -p "$th/state"
t_run "( exec 6>>'$lock'; flock -n 6 && : >'$T/held' && exec sleep 20 ) &
while [ ! -e '$T/held' ]; do sleep 0.1; done
exec 8>>'$lock'; TEHAS_UPDATE_LOCK_FD=8 '$install'; rc=\$?; kill \$!; wait; exit \$rc"
rm -f "$T/held"
t_eq "$t_rc" 1
t_contains "$t_out" "another update is running"
t_eq "$(core_files)" ""

t_case "--check runs while an update holds the lock"
t_run "exec 8>>'$lock'; flock -n 8 || exit 9; '$install' --check"
t_lacks "$t_out" "another update is running"
t_contains "$t_out" "Installed:    DRIFT"

t_case "a host without the previews' network gets it, internal and with no address for the host"
fresh
instance
t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") exit 1 ;; esac'
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(t_calls docker)" "$net_inspect
network create --internal --ipv6=false -o com.docker.network.bridge.gateway_mode_ipv4=isolated tehas-previews"
t_lacks "$t_out" "tehas-previews"
t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect"|"network create") exit 1 ;; esac'
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "The network tehas-previews could not be made"
t_stub docker "$net_good"

t_case "a network that does not keep the host out is left, and the run says why and that it has to be remade"
while IFS='|' read -r answer reason; do
  fresh
  instance
  t_stub docker "case \"\$1 \$2\" in \"version --format\") echo 27.1.0 ;; \"network inspect\") echo \"$answer\" ;; esac"
  t_run "$install"
  t_eq "$t_rc" 0
  t_eq "$(t_calls docker)" "$net_inspect"
  t_contains "$t_out" "The network tehas-previews does not keep the host out ($reason). It has to be remade"
  t_contains "$t_out" "troubleshooting"
done <<'CASES'
true bridge false |it is not isolated from the host
true bridge false nat|it is not isolated from the host
false bridge false isolated|it is not internal
true bridge true isolated|it has IPv6
true ipvlan false isolated|its driver is not bridge
CASES
t_stub docker "$net_good"

t_case "a Docker older than 27.1 is not asked for the network, and the run says why"
while IFS='|' read -r version reason; do
  fresh
  instance
  t_stub docker "case \"\$1 \$2\" in \"version --format\") echo \"$version\" ;; \"network inspect\") exit 1 ;; esac"
  t_run "$install"
  t_eq "$t_rc" 0
  t_eq "$(t_calls docker)" "version --format {{.Server.Version}}"
  t_contains "$t_out" "The network tehas-previews was not made or checked: $reason."
done <<'CASES'
26.1.4|Docker 26.1.4 is older than 27.1
27.0.3|Docker 27.0.3 is older than 27.1
|Docker's version could not be read
CASES
t_stub docker 'exit 1'
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "The network tehas-previews was not made or checked: Docker's version could not be read."
t_eq "$(t_calls docker | grep -c '^network')" 0
t_stub docker "$net_good"

t_case "--check makes no network"
fresh
instance
t_run "$install"
rm -f "$T/calls/docker"
t_run "$install --check"
t_eq "$(t_calls docker)" ""

t_case "the manual and the front page are installed"
t_eq "$(cd "$th/docs" && find . -type f ! -name version.txt | wc -l)" "$(find "$T_ROOT/host/docs" -type f | wc -l)"
t_eq "$(cd "$th/fun" && find . -type f ! -name version.txt | wc -l)" "$(find "$T_ROOT/host/fun" -type f | wc -l)"

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
for file in docs.compose.yaml router.compose.yaml; do
  for ref in $(grep -o '"\.\.\?/[A-Za-z0-9_./-]*:' "$th/edge/$file" | tr -d '":'); do
    [ -e "$th/edge/$ref" ] || missing="$missing $ref"
  done
done
t_eq "$missing" ""
t_case "a second run changes nothing"
t_run "$install"
t_contains "$t_out" "tehas is up to date"

t_case "--check on a matching install reports five lines and exits 0"
# The follow-ups of the install above are taken as done, which is what running
# tehas update does to the record. A host that still owes one is another case.
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_eq "$t_out" "Instance:     $T/instance OK
Dependencies: not checked (running inside the tests)
Slice:        not checked (running inside the tests)
Installed:    OK
Tests:        not checked (running inside the tests)"

# The slice is root's to install (sudo ./bootstrap.sh slice). install.sh only
# compares the checkout's file with the one under /etc and says what it found.
# Every container runs in the slice, so a host without it, or with another
# version of it, has something to do: the check exits 1 until it is done.
t_case "--check names a missing slice and what installs it, and exits 1"
mkdir -p "$T/etc/systemd/system"
T_ENV=(T_IN_TEST=1 T_SLICE=1 BOOTSTRAP_ETC="$T/etc")
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Slice:        MISSING (sudo ./bootstrap.sh slice)"
t_contains "$t_out" "Installed:    OK"

t_case "--check says OK for the slice file this checkout ships, and exits 0"
cp "$T_ROOT/host/slice/tehas.slice" "$T/etc/systemd/system/tehas.slice"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Slice:        OK"

t_case "--check names a slice file that differs, and exits 1"
echo "MemoryMax=10%" >>"$T/etc/systemd/system/tehas.slice"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Slice:        DIFFERS (sudo ./bootstrap.sh slice)"
t_contains "$t_out" "Installed:    OK"

t_case "a real run notes a slice that differs or is missing, and installs no slice"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "note: $T/etc/systemd/system/tehas.slice differs from host/slice/tehas.slice in this checkout (sudo ./bootstrap.sh slice)"
rm "$T/etc/systemd/system/tehas.slice"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "note: $T/etc/systemd/system/tehas.slice is missing (sudo ./bootstrap.sh slice)"
t_eq "$(find "$T/etc" -type f | wc -l)" 0
t_eq "$(installed_files | grep -c tehas.slice)" 0

t_case "a real run says nothing of a slice that is in place"
cp "$T_ROOT/host/slice/tehas.slice" "$T/etc/systemd/system/tehas.slice"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "note:"
t_lacks "$t_out" "tehas.slice"
rm -rf "${T:?}/etc"
T_ENV=(T_IN_TEST=1)

t_case "--check reports drift, changes nothing and exits 1"
echo "# edited on the host" >>"$th/libexec/dispatch"
before=$(installed_files | xargs -I{} sh -c "cd '$home' && cksum '{}'")
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Installed:    DRIFT (1 files)"
t_contains "$t_out" "differs: $th/libexec/dispatch"
t_eq "$(installed_files | xargs -I{} sh -c "cd '$home' && cksum '{}'")" "$before"

t_case "a copy that fails halfway leaves the installed file as it was"
before=$(cksum <"$th/libexec/dispatch")
# An install that writes part of the file where it was told to, then fails.
t_stub install 'head -c 100 "$3" >"$4"; exit 1'
t_run "$install"
rm -f "$T/bin/install"
t_eq "$t_rc" 1
t_contains "$t_out" "tehas: could not install $th/libexec/dispatch"
t_eq "$(cksum <"$th/libexec/dispatch")" "$before"
t_eq "$(find "$th/libexec" -name '*.new' | wc -l)" 0

t_case "a real run repairs drift and keeps the old file as .prev"
t_run "$install"
t_contains "$t_out" "installed: $th/libexec/dispatch"
t_eq "$(cmp -s "$th/libexec/dispatch" "$T_ROOT/libexec/dispatch" && echo same)" "same"
t_contains "$(cat "$th/libexec/dispatch.prev")" "# edited on the host"

t_case "a changed versions.env asks for bootstrap.sh user on a later line"
cp "$th/versions.env" "$T/versions.env.kept"
echo "# an older pin" >>"$th/versions.env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: $T_ROOT/bootstrap.sh user   (versions.env changed"
t_lacks "$t_out" "next: bootstrap"
t_run "$install"
t_lacks "$t_out" "bootstrap.sh user"
cmp -s "$th/versions.env" "$T/versions.env.kept" || t_bad "versions.env was not put back"

t_case "the console is built once, from the lock file, and installed as a program"
t_eq "$(t_calls cargo)" "build --release --locked --quiet --manifest-path $T_ROOT/console/Cargo.toml --target-dir $T/target"
t_eq "$(stat -c %a "$th/libexec/console")" 755
t_eq "$(cat "$th/libexec/console")" "a console built from source"

t_case "a console built from other source is drift, and --check builds nothing"
sed -i 's/^[0-9a-f]*/0000/' "$th/libexec/console.stamp"
rm -f "$T/calls/cargo"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Installed:    DRIFT (1 files)"
t_contains "$t_out" "differs: $th/libexec/console"
t_eq "$(t_calls cargo)" ""

t_case "a real run builds it again"
t_env CONSOLE_BUILD="new source"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "installed: $th/libexec/console"
t_eq "$(cat "$th/libexec/console")" "a console built from new source"
t_eq "$(cat "$th/libexec/console.prev")" "a console built from source"
t_eq "$(t_calls cargo | wc -l)" 1
t_run "$install --check"
t_contains "$t_out" "Installed:    OK"

t_case "a console changed on the host is drift though its stamp is as it was"
echo "# another program" >"$th/libexec/console"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "differs: $th/libexec/console"
t_run "$install"
t_contains "$t_out" "installed: $th/libexec/console"
t_eq "$(cat "$th/libexec/console")" "a console built from new source"
t_eq "$(cat "$th/libexec/console.prev")" "# another program"
t_run "$install --check"
t_contains "$t_out" "Installed:    OK"
t_env

t_case "the build is given no variable named like a secret"
fresh
instance
t_stub cargo 'env >"$T/cargo.env"
mkdir -p "$CARGO_TARGET_DIR/release" && echo built >"$CARGO_TARGET_DIR/release/console"'
t_env GH_TOKEN=hidden-token DEPLOY_SECRET=hidden-secret DB_PASSWORD=hidden-word SOME_API_KEY=hidden-key PLAIN=kept
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$(cat "$T/cargo.env")" "hidden-"
t_contains "$(cat "$T/cargo.env")" "PLAIN=kept"
t_stub_cargo
t_env

for failure in CARGO_RC=101 NO_CONSOLE=1; do
  t_case "a console that does not build installs nothing and exits 1 ($failure)"
  fresh
  instance
  # What an earlier build left is not taken for this one's.
  mkdir -p "$T/target/release"
  echo "an earlier build" >"$T/target/release/console"
  t_env "$failure"
  t_run "$install"
  t_eq "$t_rc" 1
  t_contains "$t_out" "the console did not build"
  t_eq "$(host_files | wc -l)" 0
done
t_env

t_case "a host without cargo is told so, and nothing is installed"
fresh
instance
t_env CARGO_RC=127
t_run "$install"
t_eq "$t_rc" 1
t_contains "$t_out" "cargo is not installed"
t_lacks "$t_out" "the console did not build"
t_eq "$(host_files | wc -l)" 0
t_env

t_case "the footers are given the checkout's newest tag and its commit"
fresh
instance
tagged=$T/tagged
rm -rf "$tagged"
mkdir -p "$tagged"
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$T_ROOT/libexec" "$tagged/"
t_console_source "$tagged"
t_run "$tagged/install.sh"
t_eq "$t_rc" 0
# Not a repository of its own: no version, and the pages show none.
t_eq "$(find "$th/docs" "$th/fun" -name version.txt | wc -l)" 0
commit() { git -C "$tagged" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m "$1"; }
git init -q -b main "$tagged"
commit first
t_run "$tagged/install.sh"
t_eq "$(cat "$th/docs/version.txt")" "$(git -C "$tagged" rev-parse --short=7 HEAD)"
git -C "$tagged" tag v9.8.7
commit second
t_run "$tagged/install.sh"
t_eq "$t_rc" 0
t_eq "$(cat "$th/docs/version.txt")" "v9.8.7+$(git -C "$tagged" rev-parse --short=7 HEAD)"
t_eq "$(cat "$th/fun/version.txt")" "$(cat "$th/docs/version.txt")"
t_contains "$t_out" "version: v9.8.7+"
# The file is the host's own: not stale, and no drift.
t_run "$tagged/install.sh --check"
t_contains "$t_out" "Installed:    OK"
t_lacks "$t_out" "version.txt"
for page in "$T_ROOT/host/docs/_layout.html" "$T_ROOT/host/fun/index.html"; do
  t_contains "$(cat "$page")" 'readFile "/version.txt"'
done

t_case "a checkout without the console's source installs nothing and exits 1"
fresh
instance
bare=$T/bare
rm -rf "$bare"
mkdir -p "$bare"
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$T_ROOT/libexec" "$bare/"
t_run "$bare/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "$bare/console is missing from this checkout"
t_eq "$(host_files | wc -l)" 0

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

t_case "--check names a budget setting of an earlier version, and exits 78"
instance TEHAS_WORK_MEMORY=8g
t_run "$install --check"
t_eq "$t_rc" 78
t_contains "$t_out" "INVALID"
t_contains "$t_out" "TEHAS_WORK_MEMORY is set and this version of tehas has no memory budget to set"
t_lacks "$t_out" "NOT OK"

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

t_case "an instance that asks for database clones installs nothing and exits 78"
fresh
instance TEHAS_CLONES=1
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_CLONES is set and this version of tehas has no database clones"
t_eq "$(find "$home" -type f | wc -l)" 0

t_case "an instance that asks for previews without a domain installs nothing and exits 78"
fresh
instance TEHAS_PREVIEWS=1
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_PREVIEWS is 1 and TEHAS_DOMAIN is not set"
t_eq "$(find "$home" -type f | wc -l)" 0

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

# Before bin/tehas there was one file for each command in ~/tehas/bin.
old_layout() {
  for f in tehas-console.ts tehas-dispatch tehas-energy tehas-health tehas-index tehas-protolab \
    tehas-status tehas-test tehas-work; do echo "# old" >"$th/bin/$f"; done
  echo "# older" >"$th/bin/tehas-dispatch.prev"
}

t_case "--check names the commands of the earlier layout and removes nothing"
fresh
instance
t_run "$install"
old_layout
echo "# a tool bootstrap.sh put there" >"$th/bin/gh"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Installed:    DRIFT (9 files)"
t_eq "$(printf '%s\n' "$t_out" | grep -c "stale: $th/bin/tehas-")" 9
t_contains "$t_out" "stale: $th/bin/tehas-dispatch"
t_eq "$([ -e "$th/bin/tehas-dispatch" ] && echo there)" "there"

t_case "a real run removes them, and nothing else in bin"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/bin/tehas-dispatch"
t_contains "$t_out" "removed: $th/bin/tehas-test"
# A running sampler is the old file calling itself. The units step restarts it,
# so the step is asked for and owed although no unit file changed.
t_contains "$t_out" "next: units tehas-dispatch.timer tehas-energy.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer"
t_contains "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer tehas-energy.service"
# systemd reloaded before the files its old units name were removed.
t_eq "$(t_calls systemctl)" "--user daemon-reload"
t_eq "$(cd "$th/bin" && find . -type f | LC_ALL=C sort | tr '\n' ' ')" "./gh ./tehas ./tehasbot "
t_run "$install"
t_contains "$t_out" "tehas is up to date"

t_case "the console script of before it was compiled goes the same way"
echo "// old" >"$th/libexec/console.ts"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/libexec/console.ts"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/libexec/console.ts"
t_eq "$([ -e "$th/libexec/console.ts" ] || echo gone)" "gone"
t_eq "$([ -x "$th/libexec/console" ] && echo there)" "there"

t_case "while a cycle runs they stay, and the install says to come back"
old_layout
mkdir -p "$th/state"
lock=$th/state/dispatch.lock
bash -c 'exec 8>"$1"; flock 8; exec sleep 60' _ "$lock" &
holder=$!
for _ in $(seq 50); do flock -n "$lock" true || break; sleep 0.1; done
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again when the running cycle has ended"
# Not a step: `tehas update` must not take it for one it does not know.
t_lacks "$t_out" "next: ./install.sh"
t_lacks "$t_out" "removed: $th/bin/tehas-dispatch"
t_eq "$([ -e "$th/bin/tehas-status" ] && echo there)" "there"
kill "$holder" 2>/dev/null
wait "$holder" 2>/dev/null
t_run "$install"
t_contains "$t_out" "removed: $th/bin/tehas-status"
t_eq "$([ -e "$th/bin/tehas-status" ] || echo gone)" "gone"
t_eq "$(flock -n "$lock" true && echo free)" "free"

t_case "they stay while systemd has not reloaded its units"
old_layout
rm -f "$T"/calls/systemctl
t_stub systemctl 'echo "Failed to connect to bus: No such file or directory" >&2; exit 1'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again once systemctl --user daemon-reload works"
t_lacks "$t_out" "removed: $th/bin/tehas-dispatch"
t_eq "$([ -e "$th/bin/tehas-dispatch" ] && echo there)" "there"
# A wrapper that prints an error and still exits 0 did not reload either.
t_stub systemctl 'echo "System has not been booted with systemd"'
t_run "$install"
t_contains "$t_out" "later: ./install.sh again once systemctl --user daemon-reload works"
t_eq "$([ -e "$th/bin/tehas-dispatch" ] && echo there)" "there"
t_stub systemctl
t_run "$install"
t_contains "$t_out" "removed: $th/bin/tehas-dispatch"

t_case "an old command that cannot be removed fails the install"
old_layout
chmod a-w "$th/bin"
t_run "$install"
chmod u+w "$th/bin"
if [ "$(id -u)" = 0 ]; then
  t_eq "$t_rc" 0 # root removes it anyway
else
  t_eq "$([ "$t_rc" != 0 ] && echo failed)" "failed"
  t_eq "$([ -e "$th/bin/tehas-energy" ] && echo there)" "there"
fi
t_run "$install"
t_eq "$t_rc" 0

# The code index: built by the first real run that finds both the sandbox
# image and the product checkout, and never by --check.
t_case "the Protolab server's files of before the edge router are named, then removed"
fresh
instance
t_run "$install"
rm -f "$th/state/update-pending"
echo "# old" >"$th/edge/protolab.Caddyfile"
echo "# old" >"$th/edge/protolab.compose.yaml"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "stale: $th/edge/protolab.Caddyfile"
t_contains "$t_out" "stale: $th/edge/protolab.compose.yaml"
t_eq "$([ -e "$th/edge/protolab.Caddyfile" ] && echo there)" "there"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "removed: $th/edge/protolab.Caddyfile"
t_contains "$t_out" "removed: $th/edge/protolab.compose.yaml"
t_eq "$([ -e "$th/edge/protolab.compose.yaml" ] && echo there)" ""
t_eq "$(cat "$th/state/update-pending")" "edge --force-recreate"

product() {
  mkdir -p "$T/git/acme"
  [ -d "$T/git/acme/widget" ] || {
    git init -q -b main "$T/git/acme/widget"
    git -C "$T/git/acme/widget" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
  }
  git clone -q "$T/git/acme/widget" "$th/repos/widget"
}
index_runs() { t_calls docker | grep -c -e '^run .*index_repository' || true; }
# tehas index starts nothing without the slice: a host that has it.
slice_state 0.00
slice_env="TEHAS_CGROUP_ROOT=$T_CGROUP"
t_stub systemctl '[ "$1" != show ] || printf "%s\n" FragmentPath=/etc/systemd/system/tehas.slice ActiveState=active MemoryMax=8589934592'
t_stub docker 'case "$1" in
  info) echo "systemd 2" ;;
  version) echo 29.1.3 ;;
  network) echo "true bridge false isolated" ;;
  image) exit "${IMAGE_RC:-0}" ;;
  run) [ "${RUN_RC:-0}" = 0 ] || exit "$RUN_RC"; echo "{\"status\":\"indexed\",\"nodes\":1,\"edges\":2}" ;;
esac'

t_case "an install without the product checkout names the indexing step"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: $th/bin/tehas index"
t_eq "$(index_runs)" 0

t_case "an install without the sandbox image names the indexing step"
product
t_env IMAGE_RC=1 "$slice_env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: $th/bin/tehas index"
t_eq "$(index_runs)" 0

t_case "--check builds no index"
t_env
t_run "$install --check"
t_eq "$(index_runs)" 0
t_eq "$(find "$th/state/index.json" 2>/dev/null | wc -l)" 0

t_case "a failed indexing is a warning, and the install still succeeds"
t_env RUN_RC=1 "$slice_env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: the code index was not built"
t_contains "$t_out" "later: $th/bin/tehas index"

t_case "with the image and the checkout, the install builds the index"
rm -f "$T"/calls/*
t_env "$slice_env"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas index: widget indexed"
t_lacks "$t_out" "later: $th/bin/tehas index"
t_eq "$(index_runs)" 1
t_eq "$(jq -r .nodes "$th/state/index.json")" 1

t_case "a later install leaves the index alone"
t_run "$install"
t_contains "$t_out" "tehas is up to date"
t_eq "$(index_runs)" 1
t_stub docker "$net_good"
t_stub systemctl
t_env

# --- The images a cycle is given -------------------------------------------
# TEHAS_WORK_IMAGES=1 promises a cycle the images of its issue. The sandbox
# image carries a label its build writes, and says through it whether its
# pi-rukas reads them. install.sh says what it found; it is a disagreement
# between the setting and the image, so the install still succeeds.
t_env
docker_labels() { # <label value, blank for an image with no label, absent for no image>
  case "$1" in
    absent)
      t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") echo "true bridge false isolated" ;; "image inspect") exit 1 ;; esac'
      ;;
    blank)
      t_stub docker 'case "$1 $2" in "version --format") echo 29.1.3 ;; "network inspect") echo "true bridge false isolated" ;; "image inspect") exit 0 ;; esac'
      ;;
    *)
      t_stub docker "case \"\$1 \$2\" in \"version --format\") echo 29.1.3 ;; \"network inspect\") echo \"true bridge false isolated\" ;; \"image inspect\") echo \"$1\" ;; esac"
      ;;
  esac
}
img_commit=$(printf 'c%.0s' {1..40})
img_name=tehas/pi-rukas:uid$(id -u)

t_case "images on and a sandbox image that does not read them: said, and the install still succeeds"
fresh
instance TEHAS_WORK_IMAGES=1
docker_labels "$img_commit|0"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "note: TEHAS_WORK_IMAGES is 1 and the sandbox image $img_name says its pi-rukas does not read the images a cycle is given."
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Images:       NOT READ"
t_contains "$t_out" "does not read the images a cycle is given"

t_case "images on and a sandbox image that reads them: nothing is said"
fresh
instance TEHAS_WORK_IMAGES=1
docker_labels "$img_commit|1"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "TEHAS_WORK_IMAGES"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Images:"

t_case "images on and a sandbox image that says nothing names the build that records the answer"
fresh
instance TEHAS_WORK_IMAGES=1
docker_labels blank
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "note: TEHAS_WORK_IMAGES is 1 and the sandbox image $img_name does not say whether its pi-rukas reads the images a cycle is given. Rebuild it with $th/sandbox/build.sh, which records the answer."
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Images:       NOT SAID"

t_case "images on with no sandbox image yet says nothing about images"
fresh
instance TEHAS_WORK_IMAGES=1
docker_labels absent
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "TEHAS_WORK_IMAGES"

t_case "images off says nothing about images, whatever the image says"
fresh
instance
docker_labels "$img_commit|0"
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "TEHAS_WORK_IMAGES"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Images:"
t_stub docker "$net_good"
t_stub systemctl

t_stub nproc 'echo 12'
for name in example tehas-tehas; do
  t_case "the shipped instance '$name' is valid"
  fresh
  export TEHAS_INSTANCE_DIR=$T_ROOT/instances/$name
  t_run "$install --check"
  t_lacks "$t_out" "INVALID"
  t_contains "$t_out" "Instance:     $T_ROOT/instances/$name OK"
done
rm -f "$T/bin/nproc"

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
t_contains "$t_out" "next: units tehas-dispatch.timer tehas-energy.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer"
t_eq "$(t_calls systemctl)" ""
t_contains "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer"

t_case "a run that dies partway leaves the copies it made owed"
fresh
instance
# A checkout missing one source: put() stops the run with no follow-up report at
# all. The files it did copy are on the host, and what they call for was recorded
# as each of them went - so the next tehas update still runs those steps, and
# install.sh --check still says the host is not current.
part=$T/partial
rm -rf "$part"
mkdir -p "$part"
cp -R "$T_ROOT/install.sh" "$T_ROOT/lib" "$T_ROOT/host" "$T_ROOT/bin" "$part/"
t_console_source "$part"
chmod +x "$part/install.sh"
rm "$part/host/pi/mcp.json"
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "is missing from this checkout"
t_lacks "$t_out" "next: "
t_eq "$(cat "$th/state/update-pending")" "units tehas-dispatch.timer tehas-energy.service tehas-preview.timer tehas-preview-sweep.timer tehas-protolab.timer tehas-improve.timer
sandbox
docs --force-recreate
edge --force-recreate"

t_case "the follow-ups it prints are the ids lib/update-steps.sh defines"
fresh
instance
t_stub systemctl
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(printf '%s\n' "$t_out" | sed -n 's/^next: //p')" \
  "$(tehas_steps_for systemd/tehas-dispatch.timer sandbox/Dockerfile versions.env edge/docs.compose.yaml edge/router.compose.yaml)"
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

# --- the command root installed for the accounts that are not the tehas user ---
# bootstrap.sh dev copies host/login/tehas to /usr/local/bin/tehas and leaves
# /etc/tehas/manager naming the tehas user. install.sh cannot replace a root's
# file: it reports the difference and names the command that does.
login_dev() { # what bootstrap.sh dev left: the marker and the command
  mkdir -p "$dev_etc/tehas" "$dev_bin"
  echo boss >"$dev_etc/tehas/manager"
  install -m 755 "$T_ROOT/host/login/tehas" "$dev_bin/tehas"
}
login_state() { cksum "$dev_bin/tehas"; }
t_stub sudo

t_case "a host that never ran bootstrap.sh dev reports nothing about it"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "Login:"
t_lacks "$t_out" "bootstrap.sh dev"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Login:"
t_eq "$(t_calls sudo)" ""

t_case "install.sh puts nothing there on its own"
t_eq "$([ -e "$dev_bin" ] && echo there)" ""
t_eq "$([ -e "$dev_etc/tehas/manager" ] && echo there)" ""

t_case "a host where it matches this checkout reports nothing"
fresh
instance
login_dev
t_run "$install"
t_eq "$t_rc" 0
t_lacks "$t_out" "Login:"
t_lacks "$t_out" "bootstrap.sh dev"
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_lacks "$t_out" "Login:"

t_case "--check names a stale copy and the command that replaces it, and exits 1"
echo "# the copy an earlier bootstrap.sh dev installed" >>"$dev_bin/tehas"
before=$(login_state)
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Login:        NOT CURRENT"
t_contains "$t_out" "differs: $dev_bin/tehas"
t_contains "$t_out" "sudo $T_ROOT/bootstrap.sh dev boss"
# Not drift install.sh can repair, so it is not counted as drift.
t_contains "$t_out" "Installed:    OK"
t_eq "$(login_state)" "$before"
t_eq "$(t_calls sudo)" ""

t_case "a real install names the command, writes nothing there and records no step"
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: sudo $T_ROOT/bootstrap.sh dev boss"
# Not a step of tehas update: nothing was recorded as owed.
t_eq "$(cat "$th/state/update-pending" 2>/dev/null)" ""
t_eq "$(login_state)" "$before"
t_eq "$(t_calls sudo)" ""
# The next --check still says it, because nothing here cleared it.
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Login:        NOT CURRENT"

t_case "the command names the tehas user the marker holds"
echo someone >"$dev_etc/tehas/manager"
t_run "$install --check"
t_contains "$t_out" "sudo $T_ROOT/bootstrap.sh dev someone"
echo boss >"$dev_etc/tehas/manager"

t_case "a marker that does not hold a plain account name is not carried into the line"
echo 'boss; rm -rf /' >"$dev_etc/tehas/manager"
t_run "$install --check"
t_contains "$t_out" "sudo $T_ROOT/bootstrap.sh dev <tehas user>"
t_lacks "$t_out" "rm -rf"
echo boss >"$dev_etc/tehas/manager"

t_case "the marker there and the command gone is named as missing"
rm -f "$dev_bin/tehas"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "missing: $dev_bin/tehas"
t_contains "$t_out" "sudo $T_ROOT/bootstrap.sh dev boss"

t_done
