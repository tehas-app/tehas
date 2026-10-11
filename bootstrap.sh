#!/usr/bin/env bash
# Prepare a fresh host for tehas: everything tehas and pi-rukas need, at the
# versions in host/versions.env. Safe to run again: a tool already at its
# version is skipped, one at another version is replaced.
#
#   sudo ./bootstrap.sh root <user>   system packages, Docker, the docker
#                                     group and lingering for <user>
#   ./bootstrap.sh user               the rest, as the tehas user, under that
#                                     user's home; ends with pi-rukas's installer
#   sudo ./bootstrap.sh dev <user> [<login>...]
#                                     let other accounts on the host start a
#                                     session with "tehas dev", run as <user>
#   ./bootstrap.sh check              change nothing; one line per tool with
#                                     the version found and the one expected
#
# It does not create credentials, clone the product or install tehas itself:
# see the manual's bootstrap page for the steps around it.
#
# Supported: Ubuntu 26.04 on x86_64. It stops elsewhere.
#
# What `user` owns and will replace: Node, gh and Bun from release archives;
# Pi, ctx7 and parallel-cli through npm; Rust, vipune and oo;
# codebase-memory-mcp; the pi-rukas clone. System packages are not pinned:
# `root` installs what the distribution offers and `check` only requires them
# to be present.
#
# Exit: 0 done, or check found everything; 1 something failed or check found
# a tool missing or at another version; 64 usage.
set -euo pipefail

root=$(cd "$(dirname "$0")" && pwd)
# shellcheck source=host/versions.env
. "$root/host/versions.env"

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
  local user=${1:-}
  [ -n "$user" ] || { echo "usage: sudo ./bootstrap.sh root <user>" >&2; exit 64; }
  [ "$(id -u)" = 0 ] || die "the root part must run as root: sudo ./bootstrap.sh root $user"
  id "$user" >/dev/null 2>&1 || die "no such user: $user"
  supported
  export DEBIAN_FRONTEND=noninteractive
  apt-get update -qq
  apt-get install -y -qq "${packages[@]}"
  usermod -aG docker "$user"
  loginctl enable-linger "$user"
  systemctl enable --now docker >/dev/null
  echo "bootstrap: root part done. $user must log in again for the docker group to apply."
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
  : "${TEHAS_HOME:=$HOME/tehas}"
  export TEHAS_HOME
  export NPM_CONFIG_PREFIX=$HOME/.local CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup
  export PATH="$TEHAS_HOME/bin:$TEHAS_HOME/tools/node/bin:$HOME/.local/bin:$HOME/.bun/bin:$HOME/.cargo/bin:$PATH"
}

# First x.y.z in a command's output, or nothing.
version_of() { "$@" 2>/dev/null | grep -o -E '[0-9]+\.[0-9]+\.[0-9]+' | head -n 1 || true; }

found_node() { version_of node --version; }
found_gh() { version_of gh --version; }
found_bun() { version_of bun --version; }
found_pi() { version_of pi --version; }
found_ctx7() { version_of ctx7 --version; }
found_parallel() { version_of parallel-cli --version; }
found_rust() { version_of rustc --version; }
found_vipune() { version_of vipune version; }
found_oo() { version_of oo --version; }
found_cbm() { version_of codebase-memory-mcp --version; }
found_rukas() {
  [ -e "$HOME/.pi/agent/extensions/pi-rukas" ] || return 0
  git -C "$TEHAS_HOME/repos/pi-rukas" rev-parse HEAD 2>/dev/null || true
}

# --- check --------------------------------------------------------------------
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
pi found_pi $PI_VERSION
ctx7 found_ctx7 $CTX7_VERSION
parallel-cli found_parallel $PARALLEL_CLI_VERSION
rust found_rust $RUST_VERSION
vipune found_vipune $VIPUNE_VERSION
oo found_oo $OO_VERSION
codebase-memory-mcp found_cbm $CBM_VERSION
pi-rukas found_rukas $PI_RUKAS_COMMIT
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
npm_global() { npm install -g --no-fund --no-audit "$@" >/dev/null; }
install_pi() { npm_global --ignore-scripts "@earendil-works/pi-coding-agent@$PI_VERSION"; }
install_ctx7() { npm_global --ignore-scripts "ctx7@$CTX7_VERSION"; }
# Its postinstall script downloads the binary; with scripts off there is none.
install_parallel() { npm_global "parallel-web-cli@$PARALLEL_CLI_VERSION"; }
install_rust() {
  if command -v rustup >/dev/null 2>&1; then
    rustup toolchain install "$RUST_VERSION" --profile minimal >/dev/null
    rustup default "$RUST_VERSION" >/dev/null
  else
    fetch "https://static.rust-lang.org/rustup/archive/$RUSTUP_VERSION/x86_64-unknown-linux-gnu/rustup-init" "$RUSTUP_SHA256" "$dl/rustup-init"
    chmod +x "$dl/rustup-init"
    "$dl/rustup-init" -y --profile minimal --default-toolchain "$RUST_VERSION" --no-modify-path >/dev/null 2>&1
  fi
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
    git -C "$dir" fetch -q origin
  else
    mkdir -p "$TEHAS_HOME/repos"
    git clone -q "$PI_RUKAS_REPO" "$dir"
  fi
  git -C "$dir" checkout -q "$PI_RUKAS_COMMIT"
  (cd "$dir" && ./install.sh >"$TEHAS_HOME/tmp/pi-rukas-install.log" 2>&1) ||
    die "pi-rukas's installer failed, see $TEHAS_HOME/tmp/pi-rukas-install.log"
}

do_user() {
  [ "$(id -u)" != 0 ] || die "the user part must not run as root"
  supported
  home_paths
  for tool in docker git jq curl unzip; do
    command -v "$tool" >/dev/null 2>&1 || die "$tool is missing: run 'sudo ./bootstrap.sh root $(id -un)' first"
  done
  dl=$TEHAS_HOME/tmp/dl
  mkdir -p "$TEHAS_HOME/bin" "$TEHAS_HOME/tools" "$TEHAS_HOME/repos" "$dl" "$HOME/.local/bin"
  step node found_node "$NODE_VERSION" install_node
  step gh found_gh "$GH_VERSION" install_gh
  step bun found_bun "$BUN_VERSION" install_bun
  step pi found_pi "$PI_VERSION" install_pi
  step ctx7 found_ctx7 "$CTX7_VERSION" install_ctx7
  step parallel-cli found_parallel "$PARALLEL_CLI_VERSION" install_parallel
  step rust found_rust "$RUST_VERSION" install_rust
  step vipune found_vipune "$VIPUNE_VERSION" install_vipune
  step oo found_oo "$OO_VERSION" install_oo
  step codebase-memory-mcp found_cbm "$CBM_VERSION" install_cbm
  step pi-rukas found_rukas "$PI_RUKAS_COMMIT" install_rukas
  echo "bootstrap: user part done"
}

case "${1:-}" in
  root) do_root "${2:-}" ;;
  user) do_user ;;
  dev) shift; do_dev "$@" ;;
  check) do_check ;;
  *)
    sed -n '2,16p' "$0" | sed 's/^# \{0,1\}//'
    exit 64
    ;;
esac
