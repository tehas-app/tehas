# shellcheck shell=bash
# What a sandbox container is given, and the tree it works in. Sourced by the
# three commands that start one: tehas work (a cycle), tehas plan (a planning
# turn) and tehas dev (a manual session). It is in one place so they cannot
# drift apart; test/work_test.sh, test/plan_test.sh and test/dev_test.sh pin
# the result option by option. The container's limits and the slice it runs
# in are not made here: they come from lib/slice.sh.
#
#   tehas_sandbox_env <tree> <name> <email>   the -e options
#   tehas_sandbox_mounts <tree> [<host tree> [<runs dir>]]
#                                             the -v options
#   tehas_sandbox_hosts                       the --add-host options
#   tehas_sandbox_clone <src> <tree> <branch> make <tree>, a clone of <src>
#
# The first three print docker options separated by NUL bytes. Read them with
#   mapfile -d '' -t envs < <(tehas_sandbox_env "$tree" "$name" "$email")
#
# Source lib/config.sh and call tehas_load first: the settings are read from
# the environment it leaves.
#
# Not given to any container, and not a setting: the Docker socket, SSH keys,
# ~/.config/tehas, the product checkout, the rest of the host environment.

# The same variables as the pi-rukas wrapper sets, without the host's whole
# environment: the bot token by name, the PI_ENSEMBLE_* settings, and git's
# configuration.
#
# git inside the sandbox has no configuration of its own: no credential helper
# and no identity. Without the helper the driver commits but cannot push, and
# a cycle parks after all the work is done. Hand git the bot's token through
# gh (which reads GH_TOKEN) and an identity, as environment-level config: it
# outranks any file, reaches every agent process, and writes nothing into the
# tree.
tehas_sandbox_env() { # <tree> <git name> <git email>
  local tree=$1 name=$2 email=$3 var
  local envs=(-e GH_TOKEN -e PI_ENSEMBLE_SANDBOX_MODE=1 -e "PI_ENSEMBLE_WORKSPACE_ROOT=$tree")
  envs+=(-e GIT_CONFIG_COUNT=4
    -e "GIT_CONFIG_KEY_0=credential.https://github.com.helper" -e "GIT_CONFIG_VALUE_0="
    -e "GIT_CONFIG_KEY_1=credential.https://github.com.helper" -e "GIT_CONFIG_VALUE_1=!gh auth git-credential"
    -e "GIT_CONFIG_KEY_2=user.name" -e "GIT_CONFIG_VALUE_2=$name"
    -e "GIT_CONFIG_KEY_3=user.email" -e "GIT_CONFIG_VALUE_3=$email"
    -e GIT_TERMINAL_PROMPT=0)
  while IFS='=' read -r var _; do
    case "$var" in PI_ENSEMBLE_IMAGE | PI_ENSEMBLE_HOST_ALIASES) ;; PI_ENSEMBLE_*) envs+=(-e "$var") ;; esac
  done < <(env)
  printf '%s\0' "${envs[@]}"
}

# The same mounts as the pi-rukas wrapper, minus the Docker socket, SSH keys,
# gh's configuration and image directories. <tree> is the path inside the
# container. A cycle names the directory on the host that is mounted there and
# a transcripts directory of its own, so every cycle works at one path and
# sees no other cycle's tree or transcripts. A manual session names neither.
# The directories are made first: Docker would create a missing mount source
# as a root-owned directory.
tehas_sandbox_mounts() { # <tree> [<host tree> [<runs dir>]]
  local tree=$1 agent=$HOME/.pi/agent
  local from=${2:-$1} runs=${3:-$HOME/.pi/agent/ensemble-runs}
  mkdir -p "$runs" "$agent/sessions" "$HOME/.vipune" || return 1
  printf '%s\0' -v "$from:$tree" \
    -v "$HOME/.vipune:/home/vscode/.vipune" \
    -v "$runs:/home/vscode/.pi/agent/ensemble-runs" \
    -v "$agent/sessions:/home/vscode/.pi/agent/sessions" \
    -v "$agent/models.json:/home/vscode/.pi/agent/models.json:ro" \
    -v "$agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro" \
    -v pi-ensemble-cache:/home/vscode/.cache \
    -v pi-ensemble-bun-cache:/home/vscode/.bun
}

# Names the sandbox cannot resolve by itself, from PI_ENSEMBLE_HOST_ALIASES.
tehas_sandbox_hosts() {
  local pairs=() p
  IFS=',' read -ra pairs <<<"${PI_ENSEMBLE_HOST_ALIASES:-}"
  for p in "${pairs[@]}"; do
    if [ -n "$p" ]; then printf '%s\0' "--add-host=$p"; fi
  done
}

# Make <tree>: a clone at the tip the product checkout <src> last fetched for
# <branch>, with the product's address as origin. Nothing is fetched from the
# network. The host runs git in <tree> here and only here: once a container
# has written a tree, anything in it, hooks and git configuration included,
# may be an agent's. The caller decides whether <tree> may be made.
tehas_sandbox_clone() { # <src> <tree> <branch>
  local src=$1 tree=$2 branch=$3 base url
  base=$(git -C "$src" rev-parse -q --verify "refs/remotes/origin/$branch^{commit}") ||
    { tehas_err "$src has no origin/$branch"; return 1; }
  url=$(git -C "$src" config --get remote.origin.url) || return 1
  git init -q -b "$branch" "$tree" &&
    git -C "$tree" remote add origin "$url" &&
    git -C "$tree" fetch -q --no-tags "$src" '+refs/remotes/origin/*:refs/remotes/origin/*' &&
    git -C "$tree" symbolic-ref refs/remotes/origin/HEAD "refs/remotes/origin/$branch" &&
    git -C "$tree" checkout -q -B "$branch" "$base" &&
    git -C "$tree" branch -q --set-upstream-to="origin/$branch" "$branch"
}
