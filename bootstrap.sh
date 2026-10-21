#!/usr/bin/env bash
# Prepare a fresh host for tehas: everything tehas and pi-rukas need, at the
# versions in host/versions.env, and pi-rukas at the commit versions.env
# follows (lib/rukas.sh). Safe to run again: a tool already at its version is skipped, one at
# another version is replaced. An npm tool counts as present only when its
# pinned tree is installed too.
#
#   sudo ./bootstrap.sh root <user> [--root-only]
#                                     the user if missing, system packages,
#                                     Docker on the systemd cgroup driver, the
#                                     docker group and lingering for <user>,
#                                     the slice, then the user part as <user>
#                                     in the same run, unless --root-only
#   sudo ./bootstrap.sh slice         the systemd slice the sandbox containers
#                                     run in, /etc/systemd/system/tehas.slice
#   ./bootstrap.sh user               the rest, as the tehas user, under that
#                                     user's home; ends with pi-rukas's installer
#   sudo ./bootstrap.sh dev <user> [<login>...]
#                                     let other accounts on the host start a
#                                     session with "tehas dev", run as <user>
#   ./bootstrap.sh check              change nothing; one line per tool with
#                                     the version found and the one expected,
#                                     and one for the slice
#
# It does not create credentials, clone the product or install tehas itself:
# see the manual's bootstrap page for the steps around it.
#
# Supported: Ubuntu 26.04 on x86_64. It stops elsewhere.
#
# What `user` owns and will replace: Node, gh and Bun from release archives;
# Pi, ctx7 and parallel-cli from the npm trees pinned in host/npm; Rust, vipune
# and oo; codebase-memory-mcp; the pi-rukas clone, moved to the branch's head
# on every run. System packages are not
# pinned: `root` installs what the distribution offers and `check` only requires
# them to be present.
#
# Exit: 0 done, or check found everything; 1 something failed or check found
# a tool missing or at another version; 64 usage, or slice not run as root.
set -euo pipefail

root=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$root/host/versions.env"
# shellcheck source=lib/rukas.sh
. "$root/lib/rukas.sh"

os_release=${BOOTSTRAP_OS_RELEASE:-/etc/os-release}
packages=(docker.io docker-compose-v2 docker-buildx git jq curl openssl rsync util-linux
  shellcheck unzip xz-utils build-essential pkg-config libssl-dev)

die() { echo "bootstrap: $*" >&2; exit 1; }

supported() {
  local id="" version="" arch
  # shellcheck disable=SC1090
  [ -r "$os_release" ] && id=$(. "$os_release" && echo "${ID:-}") && version=$(. "$os_release" && echo "${VERSION_ID:-}")
  arch=$(uname -m)
  [ "$id" = ubuntu ] && [ "$version" = 26.04 ] && [ "$arch" = x86_64 ] ||
    die "this host is ${id:-unknown} ${version:-?} on $arch; bootstrap.sh supports Ubuntu 26.04 on x86_64 only"
}

# --- root ---------------------------------------------------------------------
do_root() {
  local user=${1:-} only=${2:-} rc
  [ -n "$user" ] || { echo "usage: sudo ./bootstrap.sh root <user> [--root-only]" >&2; exit 64; }
  case "$only" in "" | --root-only) ;; *) echo "usage: sudo ./bootstrap.sh root <user> [--root-only]" >&2; exit 64 ;; esac
  [ "$(id -u)" = 0 ] || die "the root part must run as root: sudo ./bootstrap.sh root $user"
  [[ $user =~ ^[a-z_][a-z0-9_-]*$ ]] || die "not a plain account name: $user"
  supported
  if ! id "$user" >/dev/null 2>&1; then
    useradd -m -s /bin/bash "$user" || die "could not make the user $user"
    echo "bootstrap: made the user $user"
  fi
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq "${packages[@]}"
  usermod -aG docker "$user"
  loginctl enable-linger "$user"
  systemctl enable --now docker >/dev/null
  docker_systemd_driver || exit 1
  # A Docker that is not on the systemd driver yet does not fail this part.
  rc=0
  slice_install || rc=$?
  case "$rc" in
    0) ;;
    3) echo "bootstrap: run sudo ./bootstrap.sh slice once Docker uses the systemd driver" ;;
    *) exit 1 ;;
  esac
  if [ "$only" = --root-only ]; then
    echo "bootstrap: root part done. Next, as $user: ./bootstrap.sh user"
    return 0
  fi
  echo "bootstrap: root part done. Running the user part as $user"
  # A new login session reads the groups again, so the user part has the
  # docker group now. A session of $user that was already open does not.
  runuser -l "$user" -c "$(printf '%q' "$root/bootstrap.sh") user" ||
    die "the user part failed; run it again as $user: $root/bootstrap.sh user"
  echo "bootstrap: done. A shell $user already had open gets the docker group at its next login."
}

# Docker on cgroup v2 with the cgroupfs driver cannot put a container in a
# slice. Name the systemd driver in daemon.json, keeping every other setting,
# and restart Docker. cgroup v1 cannot be fixed here and is left to the slice
# step to report. Returns 1 when daemon.json is not JSON or a step failed.
daemon_json=${BOOTSTRAP_ETC:-/etc}/docker/daemon.json
docker_systemd_driver() {
  local answer driver version tmp kept
  answer=$(docker info --format '{{.CgroupDriver}} {{.CgroupVersion}}' 2>/dev/null) || answer=""
  read -r driver version _ <<<"$answer" || true
  [ "$driver" != systemd ] && [ "$version" = 2 ] || return 0
  if [ -e "$daemon_json" ] && ! jq -e . "$daemon_json" >/dev/null 2>&1; then
    echo "bootstrap: $daemon_json is not JSON; it was left as it is. Add \"exec-opts\": [\"native.cgroupdriver=systemd\"] by hand and run this again" >&2
    return 1
  fi
  tmp=$(mktemp)
  { if [ -e "$daemon_json" ]; then cat "$daemon_json"; else echo '{}'; fi; } |
    jq '.["exec-opts"] = ([(.["exec-opts"] // [])[] | select(startswith("native.cgroupdriver=") | not)] + ["native.cgroupdriver=systemd"])' >"$tmp" ||
    { rm -f "$tmp"; echo "bootstrap: could not write the systemd driver into $daemon_json" >&2; return 1; }
  install -d -m 755 "$(dirname "$daemon_json")"
  kept=""
  if [ -e "$daemon_json" ]; then
    cp -p "$daemon_json" "$daemon_json.prev"
    kept=", the file before is $daemon_json.prev"
  fi
  install -m 644 "$tmp" "$daemon_json"
  rm -f "$tmp"
  systemctl restart docker || { echo "bootstrap: systemctl restart docker failed" >&2; return 1; }
  echo "bootstrap: Docker uses the systemd cgroup driver now ($daemon_json$kept)"
}

# --- slice --------------------------------------------------------------------
# One system unit, host/slice/tehas.slice, copied to /etc/systemd/system. Root
# is needed for exactly this: rootful Docker places containers under PID 1, so
# a slice of the tehas user cannot hold them. It reads nothing from the
# instance, the user's home or the network.
slice_file=$root/host/slice/tehas.slice
slice_to=${BOOTSTRAP_ETC:-/etc}/systemd/system/tehas.slice

# The slice as systemd has it: sets slice_path and slice_max. True when it is
# loaded from a file, has a memory limit, and the file has not changed since
# systemd read it. A slice systemd made up for a name in use has no file and
# no limit; a unit left over from an older file still has both.
slice_loaded() {
  local out
  slice_path="" slice_max=""
  out=$(systemctl show tehas.slice -p FragmentPath -p MemoryMax -p NeedDaemonReload 2>/dev/null) || return 1
  slice_path=$(sed -n 's/^FragmentPath=//p' <<<"$out")
  slice_max=$(sed -n 's/^MemoryMax=//p' <<<"$out")
  [ -n "$slice_path" ] && [[ $slice_max =~ ^[0-9]+$ ]] && [ "$(sed -n 's/^NeedDaemonReload=//p' <<<"$out")" = no ]
}

# Returns 0 in place, 3 when Docker cannot put a container in a slice (nothing
# was written), 1 when a step failed. Every step is checked by hand: do_root
# calls this where set -e does not reach.
slice_install() {
  local answer driver version oomd
  [ -f "$slice_file" ] || { echo "bootstrap: $slice_file is missing from this checkout" >&2; return 1; }
  answer=$(docker info --format '{{.CgroupDriver}} {{.CgroupVersion}}' 2>/dev/null) || answer=""
  if [ "$answer" != "systemd 2" ]; then
    read -r driver version _ <<<"$answer" || true
    if [ -z "$driver" ] || [ -z "$version" ]; then
      echo "bootstrap: Docker gave no answer about its cgroup driver; the slice was not written" >&2
    else
      echo "bootstrap: Docker runs containers with the $driver cgroup driver on cgroup v$version, tehas needs systemd on v2." \
        "Set \"exec-opts\": [\"native.cgroupdriver=systemd\"] in /etc/docker/daemon.json and restart Docker; the slice was not written" >&2
    fi
    return 3
  fi
  # An equal file is not enough: it is in force only once systemd has read it.
  if [ -f "$slice_to" ] && cmp -s "$slice_file" "$slice_to" && slice_loaded; then
    echo "bootstrap: slice part already in place"
    return 0
  fi
  install -D -m 644 "$slice_file" "$slice_to" ||
    { echo "bootstrap: could not write $slice_to" >&2; return 1; }
  systemctl daemon-reload || { echo "bootstrap: systemctl daemon-reload failed" >&2; return 1; }
  systemctl start tehas.slice || { echo "bootstrap: systemctl start tehas.slice failed" >&2; return 1; }
  slice_loaded || {
    echo "bootstrap: tehas.slice has no memory limit after it was started (FragmentPath=${slice_path:-none}, MemoryMax=${slice_max:-none})" >&2
    return 1
  }
  # Information only: tehas neither uses nor needs systemd-oomd.
  oomd=inactive
  [ "$(systemctl is-active systemd-oomd 2>/dev/null || true)" != active ] || oomd=active
  echo "bootstrap: slice part done. tehas.slice is loaded from $slice_path, MemoryMax $slice_max bytes. systemd-oomd: $oomd"
}

do_slice() {
  [ "$#" = 0 ] || { echo "usage: sudo ./bootstrap.sh slice" >&2; exit 64; }
  [ "$(id -u)" = 0 ] || { echo "bootstrap: the slice part must run as root: sudo ./bootstrap.sh slice" >&2; exit 64; }
  slice_install || exit 1
}

# --- dev ----------------------------------------------------------------------
# Let other accounts on the host type "tehas dev". Three things, all small:
#   - the group tehas, and the named logins in it
#   - one sudo rule: the group may run <user>'s libexec/dev-login as <user>,
#     and nothing else. That script decides the clone from the account sudo
#     was asked by and starts the session.
#   - /usr/local/bin/tehas for everyone, and /etc/tehas/manager naming <user>
# A member of the group gets a session that holds the bot's token, as <user>
# does. It gets no Docker access and no shell as <user>. Run it again to add a
# login or after an update of the platform; "gpasswd -d <login> tehas" takes
# one out.
do_dev() {
  local user=${1:-} login home rule tmp
  local etc=${BOOTSTRAP_ETC:-/etc} bin=${BOOTSTRAP_BIN:-/usr/local/bin}
  [ -n "$user" ] || { echo "usage: sudo ./bootstrap.sh dev <user> [<login>...]" >&2; exit 64; }
  shift
  [ "$(id -u)" = 0 ] || die "the dev part must run as root: sudo ./bootstrap.sh dev $user $*"
  # Everything is checked before anything is changed. The names go into a
  # sudo rule, so only plain ones are taken.
  for login in "$user" "$@"; do
    [[ $login =~ ^[a-z_][a-z0-9_-]*$ ]] || die "not a plain account name: $login"
    id "$login" >/dev/null 2>&1 || die "no such user: $login"
  done
  for login in "$@"; do
    [ "$login" != "$user" ] || die "$user is the tehas user and has the whole command already"
  done
  home=$(getent passwd "$user" | cut -d: -f6)
  [[ $home =~ ^/[A-Za-z0-9_./-]+$ ]] || die "the home directory of $user is not a plain path: ${home:-(none)}"
  for login in sudo visudo; do
    command -v "$login" >/dev/null 2>&1 || die "$login is missing"
  done
  rule=$home/tehas/libexec/dev-login
  tmp=$(mktemp)
  cat >"$tmp" <<RULE
# Written by bootstrap.sh dev. Members of the group tehas may start a session
# of "tehas dev" as $user: this one script, and nothing else.
Defaults!$rule env_keep += "COLORTERM"
%tehas ALL=($user) NOPASSWD: $rule
RULE
  visudo -cf "$tmp" >/dev/null 2>&1 || { rm -f "$tmp"; die "visudo does not accept the rule; nothing was changed"; }

  getent group tehas >/dev/null 2>&1 || groupadd tehas
  for login in "$@"; do usermod -aG tehas "$login"; done
  install -d -m 755 "$etc/tehas" "$etc/sudoers.d"
  printf '%s\n' "$user" >"$etc/tehas/manager"
  chmod 644 "$etc/tehas/manager"
  install -m 440 "$tmp" "$etc/sudoers.d/tehas-dev"
  rm -f "$tmp"
  install -m 755 "$root/host/login/tehas" "$bin/tehas"
  echo "bootstrap: dev part done. Members of the group tehas can type: tehas dev"
  [ "$#" = 0 ] || echo "bootstrap: $* must log in again for the group to apply."
}

# --- shared -------------------------------------------------------------------
home_paths() {
  [ -f "$root/lib/paths.sh" ] || die "lib/paths.sh is missing from this checkout"
  # shellcheck source=lib/paths.sh
  . "$root/lib/paths.sh"
  tehas_tool_paths
}

# First x.y.z in a command's output, or nothing.
version_of() { "$@" 2>/dev/null | grep -o -E '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1 || true; }

found_node() { version_of node --version; }
found_gh() { version_of gh --version; }
found_bun() { version_of bun --version; }
found_pi() { version_of pi --version; }
found_ctx7() { version_of ctx7 --version; }
found_parallel() { version_of parallel-cli --version; }
# The MCP server is built static, so a toolchain without the musl target is
# not the one install.sh needs.
found_rust() {
  rustup target list --installed 2>/dev/null | grep -qx x86_64-unknown-linux-musl || return 0
  version_of rustc --version
}
found_vipune() { version_of vipune version; }
found_oo() { version_of oo --version; }
found_cbm() { version_of codebase-memory-mcp --version; }
found_rukas() {
  [ -e "$HOME/.pi/agent/extensions/pi-rukas" ] || return 0
  git -C "$TEHAS_HOME/repos/pi-rukas" rev-parse HEAD 2>/dev/null || true
}
# pi-rukas is not pinned: what is expected is the commit it follows now.
rukas_head() {
  local target
  target=$(tehas_rukas_target) || return 1
  printf '%s\n' "${target%% *}"
}

# --- check --------------------------------------------------------------------
# An npm tool is present as user sees it: its version on the tree versions.env
# records. Another tree at the same version is one user replaces.
found_npm() { # <tree name> <found function> <lockfile sha256>
  local found
  found=$("$2")
  [ -z "$found" ] || pin_installed "$1" "$3" || found="$found, not the pinned tree"
  printf '%s\n' "$found"
}
found_pi_tree() { found_npm pi found_pi "$PI_LOCK_SHA256"; }
found_ctx7_tree() { found_npm ctx7 found_ctx7 "$CTX7_LOCK_SHA256"; }
found_parallel_tree() { found_npm parallel-web-cli found_parallel "$PARALLEL_CLI_LOCK_SHA256"; }

do_check() {
  local rc=0 name want found tool
  home_paths
  line() { # name found expected
    local mark=ok
    [ "$2" = "$3" ] || { mark=DIFFERS; [ -n "$2" ] || mark=MISSING; rc=1; }
    printf '%-22s %-42s %-42s %s\n' "$1" "${2:--}" "$3" "$mark"
  }
  printf '%-22s %-42s %-42s %s\n' TOOL FOUND EXPECTED ""
  while read -r name found want; do
    line "$name" "$("$found")" "$want"
  done <<EOF
node found_node $NODE_VERSION
gh found_gh $GH_VERSION
bun found_bun $BUN_VERSION
pi found_pi_tree $PI_VERSION
ctx7 found_ctx7_tree $CTX7_VERSION
parallel-cli found_parallel_tree $PARALLEL_CLI_VERSION
rust found_rust $RUST_VERSION
vipune found_vipune $VIPUNE_VERSION
oo found_oo $OO_VERSION
codebase-memory-mcp found_cbm $CBM_VERSION
pi-rukas found_rukas $(rukas_head || echo "no commit for PI_RUKAS_FOLLOW=$PI_RUKAS_FOLLOW")
EOF
  # System tools: present is enough, the version is shown for the record.
  for tool in docker git jq curl openssl flock shellcheck unzip; do
    if command -v "$tool" >/dev/null 2>&1; then
      printf '%-22s %-42s %-42s %s\n' "$tool" "$(version_of "$tool" --version)" "(any)" ok
    else
      printf '%-22s %-42s %-42s %s\n' "$tool" "-" "(any)" MISSING
      rc=1
    fi
  done
  check() { # name command...
    local name=$1
    shift
    if "$@" >/dev/null 2>&1; then
      printf '%-22s %-42s %-42s %s\n' "$name" yes yes ok
    else
      printf '%-22s %-42s %-42s %s\n' "$name" no yes MISSING
      rc=1
    fi
  }
  check "docker compose" docker compose version
  check "docker buildx" docker buildx version
  check "docker without sudo" docker info
  lingering() { [ "$(loginctl show-user "$(id -un)" -p Linger --value)" = yes ]; }
  check "lingering" lingering
  # Root's to install, so it is reported and does not fail the check.
  if [ ! -f "$slice_to" ]; then
    echo "tehas.slice: missing (sudo ./bootstrap.sh slice)"
  elif cmp -s "$slice_file" "$slice_to"; then
    echo "tehas.slice: in place"
  else
    echo "tehas.slice: differs (sudo ./bootstrap.sh slice)"
  fi
  return "$rc"
}

# --- user ---------------------------------------------------------------------
# fetch <url> <sha256> <file>: download and verify; a kept download is reused.
fetch() {
  local url=$1 sha=$2 to=$3
  if [ ! -f "$to" ] || ! echo "$sha  $to" | sha256sum -c --status; then
    curl -fsSL "$url" -o "$to"
  fi
  echo "$sha  $to" | sha256sum -c --status || die "checksum mismatch for $(basename "$to")"
}

# step <name> <found function> <expected> <install function>
step() {
  local name=$1 found
  found=$("$2")
  if [ "$found" = "$3" ]; then
    echo "bootstrap: $name $3 is present"
    return 0
  fi
  echo "bootstrap: installing $name $3${found:+ (replacing $found)}"
  "$4"
  found=$("$2")
  [ "$found" = "$3" ] || die "$name is ${found:-missing} after installing, expected $3"
}

install_node() {
  fetch "https://nodejs.org/dist/v$NODE_VERSION/node-v$NODE_VERSION-linux-x64.tar.xz" "$NODE_SHA256" "$dl/node.tar.xz"
  rm -rf "$TEHAS_HOME/tools/node"
  mkdir -p "$TEHAS_HOME/tools/node"
  tar -xJf "$dl/node.tar.xz" -C "$TEHAS_HOME/tools/node" --strip-components=1
}
install_gh() {
  fetch "https://github.com/cli/cli/releases/download/v$GH_VERSION/gh_${GH_VERSION}_linux_amd64.tar.gz" "$GH_SHA256" "$dl/gh.tar.gz"
  tar -xzf "$dl/gh.tar.gz" -C "$dl"
  install -m 755 "$dl/gh_${GH_VERSION}_linux_amd64/bin/gh" "$TEHAS_HOME/bin/gh"
}
install_bun() {
  fetch "https://github.com/oven-sh/bun/releases/download/bun-v$BUN_VERSION/bun-linux-x64.zip" "$BUN_SHA256" "$dl/bun.zip"
  unzip -qo "$dl/bun.zip" -d "$dl"
  mkdir -p "$HOME/.bun/bin"
  install -m 755 "$dl/bun-linux-x64/bun" "$HOME/.bun/bin/bun"
}
# --- npm pins ---------------------------------------------------------------
# An npm tool is installed from a tree pinned beside versions.env:
# host/npm/<name>/package.json names one version, and package-lock.json fixes
# every package in that tree by hash. npm ci installs exactly that tree and
# refuses a package whose hash does not match the lockfile, so the tool and all
# of its dependencies are fixed. The copy lives under tools/npm and the tool's
# own commands are linked into the npm prefix's bin directory, which is on PATH.
#
# <name>|<package>|<version>|<lockfile sha256>
# Both halves of a pin have to be in versions.env. Without this loop a missing
# hash aborts on set -u with an unbound variable instead of a bootstrap: line.
for pin_key in PI_VERSION PI_LOCK_SHA256 CTX7_VERSION CTX7_LOCK_SHA256 \
  PARALLEL_CLI_VERSION PARALLEL_CLI_LOCK_SHA256; do
  [ -n "${!pin_key-}" ] ||
    die "host/versions.env has no $pin_key: an npm pin is its version and its lockfile hash together"
done

npm_pins=(
  "pi|@earendil-works/pi-coding-agent|$PI_VERSION|$PI_LOCK_SHA256"
  "ctx7|ctx7|$CTX7_VERSION|$CTX7_LOCK_SHA256"
  "parallel-web-cli|parallel-web-cli|$PARALLEL_CLI_VERSION|$PARALLEL_CLI_LOCK_SHA256"
)

pin_src() { echo "$root/host/npm/$1"; }
pin_tree() { echo "$TEHAS_HOME/tools/npm/$1"; }

# The version a pinned tree names for its package, one line each: what its
# package.json asks for, what the lockfile recorded, and what it resolved. All
# three must be the version in versions.env.
pin_versions() { # <name> <package>
  local src=$1 pkg=$2
  jq -r --arg p "$pkg" '.dependencies[$p] // "-"' "$src/package.json" 2>/dev/null
  jq -r --arg p "$pkg" '.packages as $pk
    | ($pk[""].dependencies[$p] // "-"), ($pk["node_modules/" + $p].version // "-")' \
    "$src/package-lock.json" 2>/dev/null
}

# Stop before anything is installed when a pinned tree is not the one recorded
# in versions.env: a lockfile that is not the file whose hash is there, or a
# version that is not the one here. Both mean a pin nobody changed on purpose.
check_pins() {
  local entry name pkg ver sha src found
  for entry in "${npm_pins[@]}"; do
    IFS='|' read -r name pkg ver sha <<<"$entry"
    src=$(pin_src "$name")
    [ -r "$src/package.json" ] && [ -r "$src/package-lock.json" ] ||
      die "$src is missing from this checkout: cannot install $pkg $ver"
    echo "$sha  $src/package-lock.json" | sha256sum --status -c - ||
      die "checksum mismatch for the npm pin $name: $src/package-lock.json is not the file recorded in host/versions.env"
    found=$(pin_versions "$src" "$pkg")
    [ "$found" = "$(printf '%s\n%s\n%s' "$ver" "$ver" "$ver")" ] ||
      die "the npm pin $name says $(printf '%s' "$found" | tr '\n' '/') for $pkg; host/versions.env says $ver. Change a tree and its version together"
  done
}

# Is the tree installed under tools/npm the one versions.env records? npm ci
# installs from a copy of the pinned lockfile, so its hash is the whole test.
pin_installed() { # <name> <lockfile sha256>
  local dir
  dir=$(pin_tree "$1")
  [ -r "$dir/package-lock.json" ] || return 1
  echo "$2  $dir/package-lock.json" | sha256sum --status -c -
}

# Install one pinned tree and put the commands of its package on PATH. The new
# tree is built in a staging directory beside the installed one and moved into
# place only once npm ci succeeded and every command it names is there: an
# install that fails - a lockfile out of sync with its package.json, a registry
# that did not answer - leaves the tree that worked and the links into it. The
# tree an earlier bootstrap left in npm's global tree is removed last, and only
# when the pinned commands are already linked.
npm_ci_pin() { # <name> <package> [npm flags...]
  local name=$1 pkg=$2 src dir new old bins bin
  shift 2
  src=$(pin_src "$name") dir=$(pin_tree "$name") new=$dir.new old=$dir.old
  rm -rf "$new"
  mkdir -p "$new"
  install -m 644 "$src/package.json" "$src/package-lock.json" "$new/"
  (cd "$new" && npm ci --no-fund --no-audit "$@" >/dev/null) || {
    rm -rf "$new"
    die "npm ci installed nothing for $pkg from $src: $dir is as it was"
  }
  bins=$(jq -r --arg p "$pkg" '(.bin // empty)
      | if type == "object" then keys_unsorted[] else ($p | split("/") | last) end' \
      "$new/node_modules/$pkg/package.json" 2>/dev/null) ||
    die "npm ci left no package.json for $pkg in $new"
  [ -n "$bins" ] || die "$pkg names no command in $new"
  while IFS= read -r bin; do
    [ -n "$bin" ] || continue
    # A leading dot lets .. name a directory, which the -x test below accepts.
    [[ $bin =~ ^[A-Za-z0-9][A-Za-z0-9._-]*$ ]] || die "$pkg names an unusable command: $bin"
    [ -x "$new/node_modules/.bin/$bin" ] || die "$pkg provides no $bin in $new/node_modules/.bin"
  done <<<"$bins"
  rm -rf "$old"
  if [ -e "$dir" ]; then mv "$dir" "$old"; fi
  mv "$new" "$dir" || {
    if [ -e "$old" ]; then mv "$old" "$dir"; fi
    die "could not move the installed tree into $dir"
  }
  rm -rf "$old"
  while IFS= read -r bin; do
    [ -n "$bin" ] || continue
    ln -sfn "$dir/node_modules/.bin/$bin" "$HOME/.local/bin/$bin"
  done <<<"$bins"
  # What an earlier bootstrap left in npm's global tree is not used again.
  rm -rf "$HOME/.local/lib/node_modules/$pkg"
}

install_pi() { npm_ci_pin pi "@earendil-works/pi-coding-agent" --ignore-scripts; }
install_ctx7() { npm_ci_pin ctx7 ctx7 --ignore-scripts; }
# Its postinstall script downloads the binary; with scripts off there is none.
install_parallel() { npm_ci_pin parallel-web-cli parallel-web-cli; }
install_rust() {
  if command -v rustup >/dev/null 2>&1; then
    rustup toolchain install "$RUST_VERSION" --profile minimal >/dev/null
    rustup default "$RUST_VERSION" >/dev/null
  else
    fetch "https://static.rust-lang.org/rustup/archive/$RUSTUP_VERSION/x86_64-unknown-linux-gnu/rustup-init" "$RUSTUP_SHA256" "$dl/rustup-init"
    chmod +x "$dl/rustup-init"
    "$dl/rustup-init" -y --profile minimal --default-toolchain "$RUST_VERSION" --no-modify-path >/dev/null 2>&1
  fi
  rustup target add x86_64-unknown-linux-musl --toolchain "$RUST_VERSION" >/dev/null
}
install_vipune() { cargo install vipune --version "$VIPUNE_VERSION" --locked --quiet; }
install_oo() { cargo install double-o --version "$OO_VERSION" --locked --quiet; }
install_cbm() {
  # Its installer refuses a group- or world-writable install directory, and
  # the default umask on Ubuntu leaves these group-writable.
  mkdir -p "$HOME/.local/bin" "$HOME/.local/lib"
  chmod go-w "$HOME/.local" "$HOME/.local/bin" "$HOME/.local/lib"
  # The installer script from the release's tag, verified here; it in turn
  # checks the archive against the release's own checksum list.
  fetch "https://raw.githubusercontent.com/DeusData/codebase-memory-mcp/v$CBM_VERSION/install.sh" "$CBM_INSTALLER_SHA256" "$dl/cbm-install.sh"
  # --skip-config: pi-rukas registers the server with Pi; nothing else is configured.
  CBM_DOWNLOAD_URL="https://github.com/DeusData/codebase-memory-mcp/releases/download/v$CBM_VERSION" \
    bash "$dl/cbm-install.sh" --skip-config >/dev/null
}
install_rukas() {
  local dir=$TEHAS_HOME/repos/pi-rukas
  if [ -d "$dir/.git" ]; then
    # A clone made before the repository changed still points at the old one.
    git -C "$dir" remote set-url origin "$PI_RUKAS_REPO"
    git -C "$dir" fetch -q --tags origin
  else
    mkdir -p "$TEHAS_HOME/repos"
    git clone -q "$PI_RUKAS_REPO" "$dir"
  fi
  git -C "$dir" checkout -q "$rukas_want"
  # A login shell exports PI_ENSEMBLE_IMAGE as the tehas sandbox image. The
  # installer would pull that name, fail, and build upstream's image over it.
  (cd "$dir" && env -u PI_ENSEMBLE_IMAGE ./install.sh >"$TEHAS_HOME/tmp/pi-rukas-install.log" 2>&1) ||
    die "pi-rukas's installer failed, see $TEHAS_HOME/tmp/pi-rukas-install.log"
}

# step for a pinned tool: present means the version is right and the tree under
# tools/npm is the one versions.env records. A host that already had the version
# from an unpinned bootstrap has to get the tree, or the pin never lands there.
step_npm() { # <name> <found function> <version> <lockfile sha256> <install function>
  local name=$1 found
  found=$("$2")
  if [ "$found" = "$3" ] && pin_installed "$1" "$4"; then
    echo "bootstrap: $name $3 is present"
    return 0
  fi
  if [ "$found" = "$3" ]; then
    echo "bootstrap: reinstalling $name $3 from host/npm/$1: $(pin_tree "$1") is not the pinned tree"
  else
    echo "bootstrap: installing $name $3${found:+ (replacing $found)}"
  fi
  "$5"
  found=$("$2")
  [ "$found" = "$3" ] || die "$name is ${found:-missing} after installing, expected $3"
  pin_installed "$1" "$4" ||
    die "$name is at $3 but $(pin_tree "$1") is not the tree recorded in host/versions.env"
}

do_user() {
  [ "$(id -u)" != 0 ] || die "the user part must not run as root"
  supported
  home_paths
  for tool in docker git jq curl unzip; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool is missing: run 'sudo ./bootstrap.sh root $(id -un)' first"
  done
  dl=$TEHAS_HOME/tmp/dl
  check_pins
  mkdir -p "$TEHAS_HOME/bin" "$TEHAS_HOME/tools" "$TEHAS_HOME/repos" "$dl" "$HOME/.local/bin"
  step node found_node "$NODE_VERSION" install_node
  step gh found_gh "$GH_VERSION" install_gh
  step bun found_bun "$BUN_VERSION" install_bun
  step_npm pi found_pi "$PI_VERSION" "$PI_LOCK_SHA256" install_pi
  step_npm ctx7 found_ctx7 "$CTX7_VERSION" "$CTX7_LOCK_SHA256" install_ctx7
  step_npm parallel-web-cli found_parallel "$PARALLEL_CLI_VERSION" "$PARALLEL_CLI_LOCK_SHA256" install_parallel
  step rust found_rust "$RUST_VERSION" install_rust
  step vipune found_vipune "$VIPUNE_VERSION" install_vipune
  step oo found_oo "$OO_VERSION" install_oo
  step codebase-memory-mcp found_cbm "$CBM_VERSION" install_cbm
  rukas_want=$(rukas_head) || die "no pi-rukas commit for PI_RUKAS_FOLLOW=$PI_RUKAS_FOLLOW in $PI_RUKAS_REPO"
  step pi-rukas found_rukas "$rukas_want" install_rukas
  echo "bootstrap: user part done"
}

case "${1:-}" in
  root) do_root "${2:-}" "${3:-}" ;;
  slice) shift; do_slice "$@" ;;
  user) do_user ;;
  dev) shift; do_dev "$@" ;;
  check) do_check ;;
  *)
    sed -n '2,24p' "$0" | sed 's/^# \{0,1\}//'
    exit 64
    ;;
esac
