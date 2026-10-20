# shellcheck shell=bash
# The home of each tool this host installs, and the PATH that finds them. One
# list, so bootstrap.sh and install.sh cannot drift: install.sh is often run by
# a program or from an update, with no login shell, and the env.sh that gives a
# login its PATH is one of the files it installs. Sourced, never run.
#
#   tehas_tool_paths
#       Export the homes and put the tools' bin directories first on PATH, in
#       the order host/env.sh uses:
#         TEHAS_HOME/bin          the commands (default TEHAS_HOME: $HOME/tehas)
#         TEHAS_HOME/tools/node   Node, installed by bootstrap.sh
#         $HOME/.local            the npm tools (NPM_CONFIG_PREFIX)
#         $HOME/.bun              Bun
#         $HOME/.cargo            Rust (CARGO_HOME; RUSTUP_HOME is $HOME/.rustup)
#       Sets nothing else, calls nothing and writes nothing.

tehas_tool_paths() {
  : "${TEHAS_HOME:=$HOME/tehas}"
  export TEHAS_HOME
  export NPM_CONFIG_PREFIX=$HOME/.local CARGO_HOME=$HOME/.cargo RUSTUP_HOME=$HOME/.rustup
  export PATH="$TEHAS_HOME/bin:$TEHAS_HOME/tools/node/bin:$HOME/.local/bin:$HOME/.bun/bin:$HOME/.cargo/bin:$PATH"
}
