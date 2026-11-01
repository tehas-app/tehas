#!/usr/bin/env bash
# The first run of a release that moved tehas update into the console comes from
# the updater of the release before it, in bash, and that one must go on working
# with what this tree installs: it runs this install.sh, reads this
# lib/update-steps.sh again after it, and keeps the functions that file no longer
# holds as it loaded them. test/fixtures/update-before is that updater, v0.20.1.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck disable=SC2154  # t_out and the *_why are set by test/lib.sh
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/update_lib.sh
. "$(dirname "$0")/update_lib.sh" || exit 1

before=$T_ROOT/test/fixtures/update-before
# A home that runs the updater from before, whatever the case made of the home.
old_updater() { # <directory that has libexec/ and lib/ under it>
  mkdir -p "$1/libexec" "$1/lib"
  cp "$before/libexec/update" "$1/libexec/update"
  cp "$before"/lib/*.sh "$T_ROOT/lib/config.sh" "$1/lib/"
}
t_cycle || t_bad "no tehas-cycle: $t_cycle_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE" CYCLE_BIN="$T_CYCLE")

t_case "the updater from before runs this install.sh and finishes what it asked for"
instance_models
t_stub gh
real_platform
old=$T/old
old_updater "$old"
t_run "mkdir -p '$th/state'; : >'$th/state/first-install'; '$old/libexec/update'"
t_eq "$t_rc" 0
t_lacks "$t_out" "command not found"
t_lacks "$t_out" "WARN"
t_contains "$t_out" "installed: $th/libexec/update"
t_eq "$(step_order)" "systemd: restarted
sandbox: building
docs: updating
edge: updating
labels: making"
t_eq "$(owed_steps)" ""

t_case "the update after that one runs from what this install.sh installed"
t_run "'$th/libexec/update'"
t_eq "$t_rc" 0
t_contains "$t_out" "tehas is up to date"
t_eq "$(owed_steps)" ""

t_case "a step the updater from before cannot run stays owed for the update after it"
instance_models
real_platform
old_updater "$old"
t_stub systemctl 'if [ "$2" = daemon-reload ]; then echo "no bus" >&2; exit 1; fi'
t_run "mkdir -p '$th/state'; : >'$th/state/first-install'; '$old/libexec/update'"
t_eq "$t_rc" 1
t_contains "$(owed_steps)" "units tehas-dispatch.timer"
t_stub systemctl 'if [ "$2" = is-active ]; then echo active; fi'
t_run "'$th/libexec/update'"
t_eq "$t_rc" 0
t_contains "$t_out" "follow-up(s) an earlier run did not finish"
t_contains "$t_out" "systemd: restarted tehas-dispatch.timer"
t_eq "$(owed_steps)" ""

# --- from a release -----------------------------------------------------------
rel=$T/releases
machine=$(uname -m)
case "$machine" in amd64) machine=x86_64 ;; arm64) machine=aarch64 ;; esac

make_release() { # <tag>
  local tag=$1 src=$T/src/tehas-$1 out=$rel/$1 name
  name=tehas-$tag-linux-$machine.tar.gz
  rm -rf "$src" "$out"
  mkdir -p "$src/instances" "$src/host/release" "$out"
  cp -R "$T_ROOT/lib" "$src/lib"
  cp -R "$T_ROOT/instances/example" "$src/instances/"
  cp "$T_ROOT/host/release/minisign.pub" "$src/host/release/"
  echo "$tag" >"$src/VERSION"
  cat >"$src/install.sh" <<'INSTALL'
#!/usr/bin/env bash
printf 'install %s\n' "${*:-}" >>"$T/calls/install"
rm -f "$TEHAS_HOME/lib/update-release.sh"
echo "next: units tehas-dispatch.timer"
echo "next: docs --force-recreate"
INSTALL
  chmod +x "$src/install.sh"
  tar -czf "$out/$name" -C "$T/src" "tehas-$tag"
  (cd "$out" && sha256sum "$name" >"$name.sha256")
  echo signed >"$out/$name.minisig"
  jq -n --arg t "$tag" '{tag_name: $t, body: "notes", draft: false, prerelease: false}' >"$out/release.json"
  jq -s '.' "$rel"/*/release.json >"$rel/list.json"
}

t_case "the updater from before takes a release of this tree and finishes it"
rm -rf "$rel" "$th"
make_release v1.0.0
make_release v1.1.0
mkdir -p "$th/versions" "$th/release" "$th/state" "$th/edge"
: >"$th/.tehas-home"
: >"$th/edge/docs.compose.yaml"
cp "$T_ROOT/host/release/minisign.pub" "$th/release/"
cp "$T_ROOT/host/versions.env" "$th/versions.env"
tar -xzf "$rel/v1.0.0/tehas-v1.0.0-linux-$machine.tar.gz" -C "$th/versions"
mv "$th/versions/tehas-v1.0.0" "$th/versions/v1.0.0"
ln -s versions/v1.0.0 "$th/current"
echo v1.0.0 >"$th/state/release-history"
old_updater "$th"
t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_FROM=release
t_stub gh 'case "$1 $2" in
  "auth status") exit 0 ;;
  "api repos/"*) cat "$T/releases/list.json" ;;
  "release download")
    tag=$3 dir=""; prev=""
    for a in "$@"; do [ "$prev" != --dir ] || dir=$a; prev=$a; done
    cp "$T/releases/$tag"/tehas-* "$dir/" ;;
esac'
t_stub minisign 'x=""; prev=""
for a in "$@"; do [ "$prev" != -x ] || x=$a; prev=$a; done
[ "$(cat "$x")" = signed ]'
t_stub docker 'if [ "$1" = ps ]; then printf "c1\n"; fi
case "$1 $2" in "image inspect") exit 1 ;; esac'
systemctl_active
t_run "'$th/libexec/update'"
t_eq "$t_rc" 0
t_lacks "$t_out" "command not found"
t_lacks "$t_out" "WARN"
t_contains "$t_out" "release: current is v1.1.0, was v1.0.0"
t_eq "$(readlink "$th/current")" versions/v1.1.0
t_contains "$(t_calls systemctl)" "--user restart tehas-dispatch.timer"
t_contains "$(t_calls docker)" "compose -f $th/edge/docs.compose.yaml up -d --force-recreate"
# What runs after the follow-ups came from a library the install took away.
t_contains "$(t_calls docker)" "tag tehas/pi-rukas:uid$(id -u) tehas/pi-rukas:release-v1.1.0"
t_eq "$([ -e "$th/lib/update-release.sh" ] && echo there)" ""
t_eq "$(owed_steps)" ""

t_done
