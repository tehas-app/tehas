#!/usr/bin/env bash
# Tests for tehas update from releases (lib/update-release.sh through
# libexec/update): which version is the target, the notes between, the two
# checks of a download, the instance checked against the target's own rules,
# the switch of current, the image a version ran with, and the last three
# versions kept. A local release directory stands in for GitHub: gh and curl
# are stubs that answer from it, and minisign is a stub that accepts a
# signature file saying "signed".
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

update="$T_ROOT/libexec/update"
th=$T/home/tehas
rel=$T/releases
T_ENV=(T_IN_TEST=1)

# A release as pack-release.sh makes it, small: the real lib/ (the target's
# rules are what is checked), the example instance, a VERSION and an install.sh
# that records it ran. Then its .sha256 and a signature the minisign stub takes.
make_release() { # <tag> [<body>]
  local tag=$1 body=${2:-} src=$T/src/tehas-$1 out=$rel/$1 name=tehas-$1-linux-x86_64.tar.gz
  rm -rf "$src" "$out"
  mkdir -p "$src/instances" "$src/host/release" "$out"
  cp -R "$T_ROOT/lib" "$src/lib"
  cp -R "$T_ROOT/instances/example" "$src/instances/"
  cp "$T_ROOT/host/release/minisign.pub" "$src/host/release/"
  echo "$tag" >"$src/VERSION"
  printf '#!/usr/bin/env bash\nprintf "install %s %%s\\n" "${*:-}" >>"$T/calls/install"\n' "$tag" >"$src/install.sh"
  chmod +x "$src/install.sh"
  tar -czf "$out/$name" -C "$T/src" "tehas-$tag"
  (cd "$out" && sha256sum "$name" >"$name.sha256")
  echo signed >"$out/$name.minisig"
  jq -n --arg t "$tag" --arg b "${body:-### Changes
- $tag
### Adopter action
None.}" '{tag_name: $t, body: $b, draft: false, prerelease: false}' >"$out/release.json"
  jq -s '.' "$rel"/*/release.json >"$rel/list.json"
}

# The host: a home installed from a release at <tag>, the installed key and
# versions.env, and an instance that updates from releases.
host_at() { # <tag>
  rm -rf "$th" "$T/calls"/* "$T/src"
  mkdir -p "$th/versions" "$th/release" "$th/state"
  : >"$th/.tehas-home"
  cp "$T_ROOT/host/release/minisign.pub" "$th/release/"
  cp "$T_ROOT/host/versions.env" "$th/versions.env"
  tar -xzf "$rel/$1/tehas-$1-linux-x86_64.tar.gz" -C "$th/versions"
  mv "$th/versions/tehas-$1" "$th/versions/$1"
  ln -s "versions/$1" "$th/current"
  echo "$1" >"$th/state/release-history"
}

# A checkout that has no release of its own: no current, so the host is taken to
# run from $TEHAS_HOME/platform. With <tag> the checkout is a git repository
# whose HEAD carries that tag.
checkout_host() { # [tag]
  host_at v1.0.0
  rm -f "$th/current"
  [ -z "${1:-}" ] || tag_checkout "$1"
}

# The platform checkout as a git repository: an empty install.sh, one commit, and
# <tag> on it.
tag_checkout() { # <tag>
  git init -q --initial-branch=main "$th/platform"
  git -C "$th/platform" config user.name test
  git -C "$th/platform" config user.email test@example.com
  : >"$th/platform/install.sh"
  git -C "$th/platform" add -A
  git -C "$th/platform" commit -qm first
  git -C "$th/platform" tag "$1"
}

instance() { t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_FROM=release "$@"; }

stubs() {
  t_stub gh 'case "$1 $2" in
  "auth status") exit "${GH_AUTH:-0}" ;;
  "api repos/"*) cat "$T/releases/list.json" ;;
  "release download")
    tag=$3 dir=""; prev=""
    for a in "$@"; do [ "$prev" != --dir ] || dir=$a; prev=$a; done
    cp "$T/releases/$tag"/tehas-* "$dir/" ;;
esac'
  t_stub curl 'out="" url=""; prev=""
for a in "$@"; do [ "$prev" != -o ] || out=$a; prev=$a; url=$a; done
case "$url" in
  https://api.github.com/*) cat "$T/releases/list.json" ;;
  */releases/download/*) tag=${url%/*}; tag=${tag##*/}; cp "$T/releases/$tag/${url##*/}" "$out" ;;
  *) exit 22 ;;
esac'
  t_stub minisign 'x=""; prev=""
for a in "$@"; do [ "$prev" != -x ] || x=$a; prev=$a; done
[ "$(cat "$x")" = signed ]'
  t_stub docker 'case "$1 $2" in "image inspect") exit "${IMAGE_RC:-1}" ;; esac'
  t_stub systemctl
}

rm -rf "$rel"
make_release v1.0.0
make_release v1.1.0 '### Changes
- the dispatcher names its reason
### Adopter action
Set TEHAS_EXAMPLE_KNOB in instance.env.'
make_release v1.2.0

t_case "--check names the installed and the newest version and the notes between, and changes nothing"
stubs
instance
host_at v1.0.0
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "release: installed v1.0.0, newest v1.2.0"
t_contains "$t_out" "== v1.1.0"
t_contains "$t_out" "Set TEHAS_EXAMPLE_KNOB in instance.env."
t_contains "$t_out" "== v1.2.0"
t_lacks "$t_out" "== v1.0.0"
t_contains "$t_out" "release: would switch to v1.2.0"
t_eq "$(readlink "$th/current")" versions/v1.0.0
t_lacks "$(t_calls gh)" "release download"
t_eq "$(t_calls install)" "install v1.0.0 --check"
t_eq "$(ls "$th/versions")" v1.0.0

t_case "an update downloads the newest, checks it, switches current and installs from it"
stubs
instance
host_at v1.0.0
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "release: v1.2.0 downloaded, checked against its .sha256 and its signature"
t_contains "$t_out" "release: current is v1.2.0, was v1.0.0"
t_eq "$(readlink "$th/current")" versions/v1.2.0
t_eq "$(cat "$th/versions/v1.2.0/VERSION")" v1.2.0
t_eq "$(t_calls install)" "install v1.2.0 "
t_contains "$(t_calls gh)" "release download v1.2.0 --repo tehas-app/tehas"
t_contains "$(t_calls docker)" "tag tehas/pi-rukas:uid$(id -u) tehas/pi-rukas:release-v1.2.0"
t_eq "$(ls "$th/tmp")" ""

t_case "the same version again downloads nothing and installs from current"
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "release: v1.2.0 is the version installed"
t_eq "$(grep -c 'release download' "$T/calls/gh")" 1

t_case "a tarball that does not match its .sha256 is not unpacked and nothing is switched"
stubs
instance
host_at v1.0.0
cp -R "$rel/v1.2.0" "$T/good"
echo tampered >>"$rel/v1.2.0/tehas-v1.2.0-linux-x86_64.tar.gz"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "does not match its .sha256"
t_contains "$t_out" "release: v1.2.0 was not installed; nothing was changed"
t_eq "$(readlink "$th/current")" versions/v1.0.0
t_eq "$(ls "$th/versions")" v1.0.0
t_eq "$(t_calls install)" ""
rm -rf "$rel/v1.2.0" && mv "$T/good" "$rel/v1.2.0"

t_case "a tarball the installed key did not sign is not unpacked"
stubs
instance
host_at v1.0.0
echo forged >"$rel/v1.2.0/tehas-v1.2.0-linux-x86_64.tar.gz.minisig"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "is not signed by the key in $th/release/minisign.pub"
t_eq "$(readlink "$th/current")" versions/v1.0.0
t_eq "$(ls "$th/versions")" v1.0.0
# The key checked with is the host's, not one in the download.
t_contains "$(t_calls minisign)" "-p $th/release/minisign.pub"
echo signed >"$rel/v1.2.0/tehas-v1.2.0-linux-x86_64.tar.gz.minisig"

t_case "with no minisign the download is refused, not taken on its .sha256 alone"
stubs
rm -f "$T/bin/minisign"
instance
host_at v1.0.0
t_run "PATH=$T/bin:/usr/bin:/bin; $update"
if command -v minisign >/dev/null 2>&1; then
  t_skip "this host has a minisign of its own"
else
  t_eq "$t_rc" 1
  t_contains "$t_out" "minisign is not installed"
  t_eq "$(readlink "$th/current")" versions/v1.0.0
fi

t_case "a release that asks for a setting the instance lacks is not switched to, and its example value is named"
stubs
make_release v1.3.0
sed -i 's/^  tehas_require TEHAS_REPO /  tehas_require TEHAS_NEW_KNOB TEHAS_REPO /' "$T/src/tehas-v1.3.0/lib/config.sh"
mkdir -p "$T/src/tehas-v1.3.0/instances/example"
echo 'TEHAS_NEW_KNOB=42' >>"$T/src/tehas-v1.3.0/instances/example/instance.env"
(cd "$T/src" && tar -czf "$rel/v1.3.0/tehas-v1.3.0-linux-x86_64.tar.gz" tehas-v1.3.0)
(cd "$rel/v1.3.0" && sha256sum tehas-v1.3.0-linux-x86_64.tar.gz >tehas-v1.3.0-linux-x86_64.tar.gz.sha256)
instance
host_at v1.0.0
t_run "$update"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_NEW_KNOB is not set"
t_contains "$t_out" "example: TEHAS_NEW_KNOB=42"
t_contains "$t_out" "settings: v1.3.0 asks for settings this instance does not have; nothing was switched"
t_eq "$(readlink "$th/current")" versions/v1.0.0
t_eq "$(t_calls install)" ""
rm -rf "$rel/v1.3.0" && jq -s '.' "$rel"/*/release.json >"$rel/list.json"

t_case "a TEHAS_ key the target release never names is warned about"
stubs
instance TEHAS_RETIRED_KNOB=1
host_at v1.0.0
# The current version knows it, so loading the settings is not refused.
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "WARN: $T/instance/instance.env sets TEHAS_RETIRED_KNOB, which v1.2.0 does not know"
t_lacks "$t_out" "sets TEHAS_UPDATE_FROM"

t_case "--to goes back to a version on disk without a download, and reuses its image"
stubs
instance
host_at v1.0.0
t_run "$update"
t_eq "$t_rc" 0
t_eq "$(readlink "$th/current")" versions/v1.2.0
: >"$T/calls/gh"
t_run "IMAGE_RC=0 $update --to v1.0.0"
t_eq "$t_rc" 0
t_contains "$t_out" "release: going back to v1.0.0 undoes these releases:"
t_contains "$t_out" "release: v1.0.0 is on this host from before; it is not downloaded again"
t_contains "$t_out" "sandbox: the image v1.0.0 ran with is in use again"
t_eq "$(readlink "$th/current")" versions/v1.0.0
t_lacks "$(t_calls gh)" "release download"
t_contains "$(t_calls docker)" "tag tehas/pi-rukas:release-v1.0.0 tehas/pi-rukas:uid$(id -u)"

t_case "--to a version that is not published changes nothing"
stubs
instance
host_at v1.0.0
t_run "$update --to v9.9.9"
t_eq "$t_rc" 1
t_contains "$t_out" "there is no published release v9.9.9"
t_eq "$(readlink "$th/current")" versions/v1.0.0

t_case "--to needs a version shaped vX.Y.Z"
t_run "$update --to latest"
t_eq "$t_rc" 64
t_contains "$t_out" "--to takes a version vX.Y.Z"
t_run "$update --to"
t_eq "$t_rc" 64

t_case "--to on a host that updates from its checkout is refused"
stubs
t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_FROM=git
host_at v1.0.0
mkdir -p "$th/platform/.git" && : >"$th/platform/install.sh"
t_run "$update --to v1.0.0"
t_eq "$t_rc" 64
t_contains "$t_out" "TEHAS_UPDATE_FROM=git"

t_case "a hold stops a plain update but not --to"
stubs
instance
host_at v1.0.0
: >"$th/state/update-hold"
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "update-hold exists"
t_run "$update --to v1.1.0"
t_eq "$t_rc" 0
t_eq "$(readlink "$th/current")" versions/v1.1.0

t_case "without the setting, current means releases; neither current nor a checkout stops with 78"
stubs
t_instance "${T_MINIMAL[@]}"
host_at v1.0.0
t_run "$update --check"
t_contains "$t_out" "release: installed v1.0.0, newest v1.2.0"
rm -f "$th/current"
t_run "$update --check"
t_eq "$t_rc" 78
t_contains "$t_out" "set TEHAS_UPDATE_FROM=release"

t_case "TEHAS_UPDATE_FROM takes git or release only"
t_instance "${T_MINIMAL[@]}" TEHAS_UPDATE_FROM=tarball
t_run "$update --check"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_UPDATE_FROM is not valid: expected git or release"
t_instance "${T_MINIMAL[@]}" TEHAS_RELEASE_REPO=no-slash
t_run "$update --check"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_RELEASE_REPO is not valid: expected owner/name"

t_case "gh not signed in: the list and the download come from the public address"
stubs
instance
host_at v1.0.0
t_run "GH_AUTH=1 $update"
t_eq "$t_rc" 0
t_contains "$(t_calls curl)" "https://api.github.com/repos/tehas-app/tehas/releases?per_page=100"
t_contains "$(t_calls curl)" "https://github.com/tehas-app/tehas/releases/download/v1.2.0/tehas-v1.2.0-linux-x86_64.tar.gz.minisig"
t_eq "$(readlink "$th/current")" versions/v1.2.0

t_case "TEHAS_RELEASE_REPO in the instance names where releases come from"
stubs
instance TEHAS_RELEASE_REPO=someone/fork
host_at v1.0.0
t_run "$update --check"
t_contains "$(t_calls gh)" "api repos/someone/fork/releases"

t_case "a checkout tagged v1.0.0 is shown only the notes after its own tag, and still would switch"
stubs
instance
checkout_host v1.0.0
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "release: installed none, this host ran from its platform checkout at v1.0.0, newest v1.2.0"
t_contains "$t_out" "== v1.1.0"
t_contains "$t_out" "Set TEHAS_EXAMPLE_KNOB in instance.env."
t_contains "$t_out" "== v1.2.0"
t_lacks "$t_out" "== v1.0.0"
t_contains "$t_out" "release: would switch to v1.2.0"

t_case "a checkout at the newest tag is shown no notes and still would switch"
stubs
make_release v1.3.0
instance
checkout_host v1.3.0
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "release: installed none, this host ran from its platform checkout at v1.3.0, newest v1.3.0"
t_lacks "$t_out" "== v1"
t_contains "$t_out" "release: would switch to v1.3.0"
rm -rf "$rel/v1.3.0" && jq -s '.' "$rel"/*/release.json >"$rel/list.json"

t_case "a checkout with no tag is shown every release, as before"
stubs
instance
checkout_host
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "release: installed none, this host ran from its platform checkout, newest v1.2.0"
t_contains "$t_out" "== v1.0.0"
t_contains "$t_out" "== v1.1.0"
t_contains "$t_out" "== v1.2.0"
t_contains "$t_out" "release: would switch to v1.2.0"

t_case "no checkout on the host is shown every release, as before"
stubs
instance
host_at v1.0.0
rm -f "$th/current"
t_run "$update --check"
t_eq "$t_rc" 1
t_contains "$t_out" "release: installed none, this host ran from its platform checkout, newest v1.2.0"
t_contains "$t_out" "== v1.0.0"
t_contains "$t_out" "release: would switch to v1.2.0"

t_case "an update from a tagged checkout at the newest release still downloads and switches"
stubs
instance
checkout_host v1.2.0
t_run "$update"
t_eq "$t_rc" 0
t_contains "$t_out" "this host ran from its platform checkout at v1.2.0"
t_lacks "$t_out" "== v1"
t_contains "$t_out" "release: current is v1.2.0, was the platform checkout"
t_eq "$(readlink "$th/current")" versions/v1.2.0
t_contains "$(t_calls gh)" "release download v1.2.0"
t_eq "$(t_calls install)" "install v1.2.0 "

t_case "the version current names and the two switched to before it stay; older ones go"
stubs
instance
host_at v1.0.0
make_release v1.3.0
make_release v1.4.0
for v in v1.1.0 v1.2.0 v1.3.0 v1.4.0; do
  t_run "$update --to $v"
  t_eq "$t_rc" 0
done
t_eq "$(cd "$th/versions" && printf '%s ' v*)" "v1.2.0 v1.3.0 v1.4.0 "
t_contains "$t_out" "release: removed v1.1.0, older than the last three"
t_contains "$(t_calls docker)" "image rm tehas/pi-rukas:release-v1.0.0"
rm -rf "$rel/v1.3.0" "$rel/v1.4.0" && jq -s '.' "$rel"/*/release.json >"$rel/list.json"

t_case "a tarball holding a member outside its top directory is refused"
stubs
instance
host_at v1.0.0
mkdir -p "$T/evil/tehas-v1.2.0" && echo v1.2.0 >"$T/evil/tehas-v1.2.0/VERSION" && echo x >"$T/evil/outside"
cp -R "$rel/v1.2.0" "$T/good"
(cd "$T/evil" && tar -czf "$rel/v1.2.0/tehas-v1.2.0-linux-x86_64.tar.gz" tehas-v1.2.0 outside)
(cd "$rel/v1.2.0" && sha256sum tehas-v1.2.0-linux-x86_64.tar.gz >tehas-v1.2.0-linux-x86_64.tar.gz.sha256)
t_run "$update"
t_eq "$t_rc" 1
t_contains "$t_out" "holds what a release does not: outside"
t_eq "$(ls "$th/versions")" v1.0.0
rm -rf "$rel/v1.2.0" && mv "$T/good" "$rel/v1.2.0"

t_done
