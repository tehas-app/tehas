# shellcheck shell=bash
# What a sandbox container is given, and the tree it works in. Sourced by the
# commands that start one: tehas work (a cycle), tehas plan (a planning turn),
# tehas dev (a manual session) and tehas index (the indexer). It is in one
# place so they cannot drift apart; test/work_test.sh, test/plan_test.sh and
# test/dev_test.sh pin the result option by option. The container's limits and
# the slice it runs in are not made here: they come from lib/slice.sh.
#
#   tehas_sandbox_env <tree> <name> <email>   the -e options
#   tehas_sandbox_settings_ok                 is Pi's settings.json there
#   tehas_sandbox_mounts <tree> <host> <state> <runs>
#                                             the -v options
#   tehas_sandbox_retry <host>                the provider retry a container's
#                                             Pi runs with, as one line
#   tehas_sandbox_seed <state> [<index dir>]  give <state> the host's code index
#   tehas_sandbox_cache_dir                   where a container has that copy
#   tehas_index_dir                           where the host keeps the index
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
# ~/.config/tehas, the product checkout, the rest of the host environment. Of
# ~/.config/tehas/env a container is given the bot's token and the model
# endpoints' tokens that models.json names, by name.

# shellcheck source=lib/models.sh
. "$(dirname "${BASH_SOURCE[0]}")/models.sh"

# Where a container has its own copy of the code index, and the variable that
# points the indexer and the agents' code search at it. It is a path inside the
# container, not a setting: no instance names it. The host directory behind it
# belongs to one container and goes with it, so nothing one container writes
# there is what a later container reads.
tehas_sandbox_cache_dir() { printf '%s\n' /home/vscode/.tehas-cache; }

# Where the host keeps the code index it gives to containers: the one directory
# the indexer writes and every container is given a copy of. No container is
# given it itself. Derived from TEHAS_HOME, not a setting.
tehas_index_dir() { printf '%s\n' "$TEHAS_HOME/state/index"; }

# The same variables as the pi-rukas wrapper sets, without the host's whole
# environment: the bot token by name, the PI_ENSEMBLE_* settings, and git's
# configuration. And the model endpoints' tokens models.json names, by name:
# Pi reads "$TEHAS_MODEL_KEY_<NAME>" from its environment.
#
# git inside the sandbox has no configuration of its own: no credential helper
# and no identity. Without the helper the driver commits but cannot push, and
# a cycle parks after all the work is done. Hand git the bot's token through
# gh (which reads GH_TOKEN) and an identity, as environment-level config: it
# outranks any file, reaches every agent process, and writes nothing into the
# tree.
tehas_sandbox_env() { # <tree> <git name> <git email>
  local tree=$1 name=$2 email=$3 var
  local envs=(-e GH_TOKEN -e PI_ENSEMBLE_SANDBOX_MODE=1 -e "PI_ENSEMBLE_WORKSPACE_ROOT=$tree"
    -e "CBM_CACHE_DIR=$(tehas_sandbox_cache_dir)")
  envs+=(-e GIT_CONFIG_COUNT=4
    -e "GIT_CONFIG_KEY_0=credential.https://github.com.helper" -e "GIT_CONFIG_VALUE_0="
    -e "GIT_CONFIG_KEY_1=credential.https://github.com.helper" -e "GIT_CONFIG_VALUE_1=!gh auth git-credential"
    -e "GIT_CONFIG_KEY_2=user.name" -e "GIT_CONFIG_VALUE_2=$name"
    -e "GIT_CONFIG_KEY_3=user.email" -e "GIT_CONFIG_VALUE_3=$email"
    -e GIT_TERMINAL_PROMPT=0)
  # Not the settings of pi-rukas's own docker wrapper, which no container
  # runs: the image, the aliases (given as --add-host) and the socket and SSH
  # switches. The runner does not list them as inputs.
  while IFS='=' read -r var _; do
    case "$var" in
      PI_ENSEMBLE_IMAGE | PI_ENSEMBLE_HOST_ALIASES | PI_ENSEMBLE_NO_DOCKER_SOCKET | PI_ENSEMBLE_NO_SSH) ;;
      PI_ENSEMBLE_*) envs+=(-e "$var") ;;
    esac
  done < <(env)
  while read -r var; do
    [ -z "$var" ] || [ -z "${!var:-}" ] || envs+=(-e "$var")
  done < <(tehas_model_key_names)
  printf '%s\0' "${envs[@]}"
}

# What one container is given, and none of what another container wrote.
# <tree> is the path inside the container and <host> the directory mounted
# there. <state> is a host directory of this container's own: its copy of the
# code index, its memory and its sessions go under it. <runs> is the host
# directory of its transcripts. All four are named by the caller: there is no
# default that lets one container's writes be another's input. The caller makes
# <host>; this makes <state> and <runs>, because Docker would create a missing
# mount source as a root-owned directory.
#
# The two cache volumes pi-rukas mounts are not mounted here. What a container
# writes under its home, caches of packages among them, stays in its own layer
# and goes when the container is removed.
#
# Pi's settings.json is the one install.sh puts in ~/.pi/agent: its provider
# retry and timeout. The image has none, and without it Pi makes no retry of a
# failed request. A missing file is a failure, not a default: Docker would
# make a root-owned directory in its place.
#
# models.json is the container's own, <state>/models.json: the host's with
# each provider on the endpoint picked now (lib/models.sh). <state>/endpoints
# says which providers are on their backup.
# Whether Pi's settings.json and models.json are there to be mounted, and
# the tokens models.json names are set. The commands ask before
# they call Docker or git or write anything; tehas_sandbox_mounts asks again.
tehas_sandbox_settings_ok() {
  local f
  for f in settings.json models.json; do
    [ -f "$HOME/.pi/agent/$f" ] || { tehas_err "no $HOME/.pi/agent/$f: run install.sh"; return 1; }
  done
  tehas_model_keys_ok
}

tehas_sandbox_mounts() { # <tree> <host> <state> <runs>
  local tree=$1 host=$2 state=$3 runs=$4 agent=$HOME/.pi/agent cache
  [ -n "$tree" ] && [ -n "$host" ] && [ -n "$state" ] && [ -n "$runs" ] || return 1
  tehas_sandbox_settings_ok || return 1
  cache=$(tehas_sandbox_cache_dir)
  mkdir -p "$runs" "$state/cache" "$state/memory" "$state/sessions" || return 1
  tehas_models_resolve "$state/models.json" >"$state/endpoints" || return 1
  printf '%s\0' -v "$host:$tree" \
    -v "$state/cache:$cache" \
    -v "$state/memory:/home/vscode/.vipune" \
    -v "$state/sessions:/home/vscode/.pi/agent/sessions" \
    -v "$runs:/home/vscode/.pi/agent/ensemble-runs" \
    -v "$state/models.json:/home/vscode/.pi/agent/models.json:ro" \
    -v "$agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro" \
    -v "$agent/settings.json:/home/vscode/.pi/agent/settings.json:ro"
}

# The provider and agent retry and timeout a container's Pi runs with: the host's
# settings.json that tehas_sandbox_mounts gives it, under the product's own
# .pi/settings.json in <host>, merged the way Pi merges them. A value neither
# sets is Pi's default. For the cycle's log, so a reader of it knows them.
tehas_sandbox_retry() { # <host>
  local global=$HOME/.pi/agent/settings.json project=$1/.pi/settings.json
  [ -f "$global" ] || { echo "provider retry: unknown, $global is missing"; return 0; }
  [ -r "$project" ] || project=/dev/null
  jq -n -r --slurpfile g "$global" --slurpfile p "$project" '
    (($g[0] // {}) * ($p[0] // {})) as $s
    | def v($x): if $x == null then "unset" else ($x | tostring) end;
    "provider retry: timeoutMs=\(v($s.retry.provider.timeoutMs))"
    + " maxRetries=\(v($s.retry.provider.maxRetries))"
    + " maxRetryDelayMs=\(v($s.retry.provider.maxRetryDelayMs))"
    + " httpIdleTimeoutMs=\(v($s.httpIdleTimeoutMs))"
    + " agentMaxRetries=\(v($s.retry.maxRetries))"
    + " agentBaseDelayMs=\(v($s.retry.baseDelayMs))"
    + " agentMaxDelayMs=\(v($s.retry.maxAgentDelayMs))"' 2>/dev/null ||
    echo "provider retry: unknown, $global or $project is not JSON"
}

# Put the host's code index in <state>/cache, the copy this container is given.
# The index is one sqlite file per tree that was indexed. Not copied: the
# indexer's own logs, which a container has no use for, and what a run that
# was stopped left behind (a staging file about half the size of the index it
# was writing, read by nothing). Without that the copy would be several times
# the index and grow with every interrupted run.
# <index dir> is tehas_index_dir unless another is named. A host that never
# indexed anything has nothing to give: that is not a failure, and a container
# then searches by reading files.
tehas_sandbox_seed() { # <state> [<index dir>]
  local state=$1 src=${2:-$(tehas_index_dir)} f
  mkdir -p "$state/cache" || return 1
  [ -d "$src" ] || return 0
  for f in "$src"/*; do
    [ -e "$f" ] || continue
    case "$f" in */logs | *.db.stage.* | *.tmp) continue ;; esac
    cp -a "$f" "$state/cache/" || {
      rm -rf "$state/cache"
      mkdir -p "$state/cache"
      return 1
    }
  done
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
# network. The host runs git in <tree> here and only here: once a container has
# written a tree, anything in it, hooks and git configuration included, may be
# an agent's. The caller decides whether <tree> may be made.
tehas_sandbox_clone() { # <src> <tree> <branch>
  local src=$1 tree=$2 branch=$3 base url
  base=$(git -C "$src" rev-parse -q --verify "refs/remotes/origin/$branch^{commit}") ||
    { tehas_err "$src has no origin/$branch"; return 1; }
  url=$(git -C "$src" config --get remote.origin.url) || return 1
  git init -q -b "$branch" "$tree" &&
    git -C "$tree" remote add origin "$url" &&
    timeout -k 10 300 git -C "$tree" fetch -q --no-tags "$src" '+refs/remotes/origin/*:refs/remotes/origin/*' &&
    git -C "$tree" symbolic-ref refs/remotes/origin/HEAD "refs/remotes/origin/$branch" &&
    git -C "$tree" checkout -q -B "$branch" "$base" &&
    git -C "$tree" branch -q --set-upstream-to="origin/$branch" "$branch"
}
