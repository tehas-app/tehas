#!/usr/bin/env bash
# The update of a host on 0.30.x to this release, by tehas update. The 0.30.x
# updater lists the releases of the repository its versions.env names,
# checks the download with the key in ~/tehas/release/minisign.pub, asks this
# release's lib/config.sh whether the instance holds, and switches current.
# From then on this release's own update runs on the home 0.30.x left, and it
# reads neither of those two files. test/fixtures/install-0.30 is what a v0.30.6
# install leaves in the home (its versions.env, its key and its VERSION, taken
# from the v0.30.6 tag) and settings an instance of 0.30.x may set that this
# release renamed or removed.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"
T_ENV_ALWAYS=(TEHAS_CONSOLE="$T_CONSOLE" GH_TOKEN=ghs_test)
T_ENV=(T_IN_TEST=1)

update="$T_ROOT/libexec/update"
old=$T_ROOT/test/fixtures/install-0.30
th=$T/home/tehas
rel=$T/releases
pinned=$(sed -n 's/^TEHAS_RELEASE_REPO=//p' "$T_ROOT/host/versions.env" | tail -n 1)
machine=$(uname -m)
case "$machine" in amd64) machine=x86_64 ;; arm64) machine=aarch64 ;; esac

# A release of this tree, small: its real lib/ and example instance, so the
# settings check is this release's, and an install.sh that records it ran.
make_release() { # <tag> [prerelease true|false]
  local tag=$1 pre=${2:-false} src=$T/src/tehas-$1 out=$rel/$1 name
  name=tehas-$tag-linux-$machine.tar.gz
  rm -rf "$src" "$out"
  mkdir -p "$src/instances" "$out"
  cp -R "$T_ROOT/lib" "$src/lib"
  cp -R "$T_ROOT/instances/example" "$src/instances/"
  echo "$tag" >"$src/VERSION"
  printf '#!/usr/bin/env bash\nprintf "install %s\\n" >>"$T/calls/install"\n' "$tag" >"$src/install.sh"
  chmod +x "$src/install.sh"
  tar -czf "$out/$name" -C "$T/src" "tehas-$tag"
  (cd "$out" && sha256sum "$name" >"$name.sha256")
  echo signed >"$out/$name.minisig"
  jq -n --arg t "$tag" --argjson p "$pre" \
    '{tag_name: $t, body: "### Changes\n- \($t)", draft: false, prerelease: $p}' >"$out/release.json"
  jq -s '.' "$rel"/*/release.json >"$rel/list.json"
}

# The home as v0.30.6 left it, running v0.30.6 from current.
home_030() {
  rm -rf "$th" "$T/calls"/*
  mkdir -p "$th/versions/v0.30.6" "$th/release" "$th/state"
  : >"$th/.tehas-home"
  cp "$old/versions.env" "$th/versions.env"
  cp "$old/release/minisign.pub" "$th/release/minisign.pub"
  cp "$old/VERSION" "$th/versions/v0.30.6/VERSION"
  ln -s versions/v0.30.6 "$th/current"
  echo v0.30.6 >"$th/state/release-history"
}

# The 0.30.x instance with each row of api/renames-1.0.tsv applied: a renamed
# setting under its new name, a removed one gone.
renamed_030() {
  awk -F'\t' 'NR == FNR { if ($1 == "setting") new[$2] = $3; next }
    { key = $0; sub(/=.*/, "", key)
      if (key in new) { if (new[key] == "") next; sub(/^[^=]*=/, new[key] "=") }
      print }' "$T_ROOT/api/renames-1.0.tsv" "$old/instance.env"
}

stubs() {
  t_stub gh 'case "$1 $2" in
  "auth status") exit 0 ;;
  "api repos/"*) cat "$T/releases/list.json" ;;
  "release download")
    tag=$3 dir=""; prev=""
    for a in "$@"; do [ "$prev" != --dir ] || dir=$a; prev=$a; done
    cp "$T/releases/$tag"/tehas-* "$dir/" ;;
esac'
  t_stub minisign 'x=""; p=""; prev=""
for a in "$@"; do [ "$prev" != -x ] || x=$a; [ "$prev" != -p ] || p=$a; prev=$a; done
cat "$p" >"$T/calls/minisign-key"
[ "$(cat "$x")" = signed ]'
  t_stub docker 'case "$1 $2" in "image inspect") exit 1 ;; esac'
  t_stub systemctl
}

# What the 0.30.x updater asks of the release before it switches current
# (settings_hold in console/ops/src/update/release.rs of v0.30.6).
settings_hold() { # <release tree>
  t_run "bash -c '. \"\$1/lib/config.sh\" && tehas_load && tehas_require_instance_settings' _ '$1'"
}

t_case "a 0.30.x host lists releases where this release is published and checks them with its key"
t_eq "$(sed -n 's/^TEHAS_RELEASE_REPO=//p' "$old/versions.env" | tail -n 1)" "$pinned"
t_eq "$(cat "$old/release/minisign.pub")" "$(cat "$T_ROOT/host/release/minisign.pub")"

t_case "this release's settings check refuses a 0.30.x instance that sets a renamed setting"
home_030
mapfile -t lines <"$old/instance.env"
t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_FROM=release "${lines[@]}"
settings_hold "$T_ROOT"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_EASY_MERGE is no longer read"
t_contains "$t_out" "$TEHAS_INSTANCE_DIR/instance.env"

t_case "with the renames applied, this release's settings check holds"
mapfile -t lines < <(renamed_030)
t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_FROM=release "${lines[@]}"
settings_hold "$T_ROOT"
t_eq "$t_rc" 0
t_contains "$(cat "$TEHAS_INSTANCE_DIR/instance.env")" "TEHAS_MAX_CYCLES=3"

t_case "this release updates the home 0.30.x left from the built-in repository and key"
rm -rf "$rel"
make_release v0.30.6
make_release v1.0.0
stubs
home_030
mapfile -t lines < <(renamed_030)
t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_FROM=release "${lines[@]}"
# What 0.30.x left in the home names another repository and another key.
sed -i 's|^TEHAS_RELEASE_REPO=.*|TEHAS_RELEASE_REPO=someone/else|' "$th/versions.env"
echo "untrusted comment: a key a release left" >"$th/release/minisign.pub"
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "release: current is v1.0.0, was v0.30.6"
t_eq "$(readlink "$th/current")" versions/v1.0.0
t_contains "$(t_calls gh)" "api repos/$pinned/releases"
t_contains "$(t_calls gh)" "release download v1.0.0 --repo $pinned"
t_lacks "$(t_calls gh)" "someone/else"
t_eq "$(cat "$T/calls/minisign-key")" "$(cat "$T_ROOT/host/release/minisign.pub")"
t_eq "$(t_calls install)" "install v1.0.0"

t_case "from the 0.30.x layout a prerelease is installed only when --to names it"
make_release v1.0.1-rc.1 true
stubs
home_030
t_run "$update"
t_eq "$t_rc" 0
t_eq "$(readlink "$th/current")" versions/v1.0.0
home_030
t_run "$update --to v1.0.1-rc.1"
t_eq "$t_rc" 0
t_eq "$(readlink "$th/current")" versions/v1.0.1-rc.1

t_done
