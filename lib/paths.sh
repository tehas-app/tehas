# shellcheck shell=bash
# The home of each tool this host installs, and the PATH that finds them. One
# list, in one function: host/env.sh sources the installed copy for a login or
# a unit, and bootstrap.sh and install.sh source this checkout's. A run of
# install.sh is often made by a program or from an update, with no login shell,
# and the env.sh that gives a login its PATH is one of the files it installs.
# Sourced, never run.
#
#   tehas_tool_paths
#       Export the homes and put the tools' bin directories first on PATH, in
#       this order, leaving one already on PATH where it is:
#         TEHAS_HOME/bin          the commands
#         TEHAS_HOME/tools/node   Node, installed by bootstrap.sh
#         NPM_CONFIG_PREFIX       the npm tools (default $HOME/.local)
#         BUN_INSTALL             Bun (default $HOME/.bun)
#         CARGO_HOME              Rust (default $HOME/.cargo; RUSTUP_HOME is
#                                 $HOME/.rustup)
#       A home already set is kept, so the caller's setting wins; the defaults
#       here are the ones host/env.sh and lib/config.sh apply for TEHAS_HOME,
#       and test/paths_test.sh pins all three to $HOME/tehas. Calling it again
#       adds nothing, which is what lets a login shell source env.sh and the
#       install or update it then runs call this too. Sets nothing else, calls
#       nothing and writes nothing.

tehas_tool_paths() {
  local dir add=""
  : "${TEHAS_HOME:=$HOME/tehas}" "${BUN_INSTALL:=$HOME/.bun}" \
    "${NPM_CONFIG_PREFIX:=$HOME/.local}" "${CARGO_HOME:=$HOME/.cargo}" \
    "${RUSTUP_HOME:=$HOME/.rustup}"
  export TEHAS_HOME BUN_INSTALL NPM_CONFIG_PREFIX CARGO_HOME RUSTUP_HOME
  for dir in "$TEHAS_HOME/bin" "$TEHAS_HOME/tools/node/bin" "$NPM_CONFIG_PREFIX/bin" \
    "$BUN_INSTALL/bin" "$CARGO_HOME/bin"; do
    case ":${PATH:-}:" in *":$dir:"*) continue ;; esac
    add=$add${add:+:}$dir
  done
  if [ -n "$add" ]; then export PATH="$add:${PATH:-}"; fi
}
