# shellcheck shell=bash
# shellcheck disable=SC2034  # the sb_* variables are read by the commands that source this file
# What a pi-rukas container is given, in one place. Sourced by bin/tehas-work
# and bin/tehas-plan after lib/config.sh has loaded the instance.
#
#   sandbox_prepare <command name> <branch>
#       Check the bot token and git identity, build the variables and make a
#       fresh workspace at the branch's tip. Sets sb_repo, sb_image, sb_agent,
#       sb_envs and sb_hosts. Exits 64 without a token or an identity, 78 when
#       the workspace cannot be made.
#   sandbox_pi <container name> <log file> [pi argument...]
#       Start `pi --mode rpc --approve` in the image, in the background. Its
#       stdin is file descriptor 3, its output goes to the log. Sets sb_pid.
#
# The list of mounts and variables is deliberate and pinned by
# test/work_test.sh and test/plan_test.sh: the bot token (pi-rukas pushes and
# files issues from inside), the PI_ENSEMBLE_* settings, the bot's git
# identity, the workspace and Pi's state directories. Not the Docker socket,
# SSH keys or ~/.config/tehas. A change here is a security change.
#
# The workspace is $TEHAS_WORK_DIR, a fresh clone for every run. The container
# never sees the product checkout: an agent can write anything into the tree
# it works in, hooks and git configuration included, and the host runs git in
# the product checkout.

sandbox_prepare() {
  local cmd=$1 branch=$2 src=$TEHAS_WORK_REPO bot_name bot_email base url works p var
  local -a pairs
  sb_repo=$TEHAS_WORK_DIR
  sb_image=$PI_ENSEMBLE_IMAGE
  sb_agent=$HOME/.pi/agent

  [ -n "${GH_TOKEN:-}" ] || { echo "$cmd: GH_TOKEN is not set (source ~/tehas/env.sh)" >&2; exit 64; }
  mkdir -p "$TEHAS_HOME/logs" "$sb_agent/ensemble-runs" "$sb_agent/sessions" "$HOME/.vipune"

  sb_hosts=()
  IFS=',' read -ra pairs <<<"${PI_ENSEMBLE_HOST_ALIASES:-}"
  for p in "${pairs[@]}"; do [ -n "$p" ] && sb_hosts+=(--add-host="$p"); done

  # The same mounts as the pi-rukas wrapper, minus the Docker socket, SSH keys
  # and image directories. Only the bot token and PI_ENSEMBLE_* settings are
  # passed in, not the whole host environment.
  sb_envs=(-e GH_TOKEN -e PI_ENSEMBLE_SANDBOX_MODE=1 -e "PI_ENSEMBLE_WORKSPACE_ROOT=$sb_repo")

  # git inside the sandbox has no configuration of its own: no credential helper
  # and no identity. Without the helper the driver commits but cannot push, and
  # the cycle parks after all the work is done. Hand git the bot's token through
  # gh (which reads GH_TOKEN) and the bot's identity, as environment-level
  # config: it outranks any file, reaches every agent process, and writes
  # nothing into the checkout.
  bot_name=$(git config --global user.name || true)
  bot_email=$(git config --global user.email || true)
  [ -n "$bot_name" ] && [ -n "$bot_email" ] ||
    { echo "$cmd: git user.name and user.email are not set for $(id -un)" >&2; exit 64; }
  sb_envs+=(-e GIT_CONFIG_COUNT=4
    -e "GIT_CONFIG_KEY_0=credential.https://github.com.helper" -e "GIT_CONFIG_VALUE_0="
    -e "GIT_CONFIG_KEY_1=credential.https://github.com.helper" -e "GIT_CONFIG_VALUE_1=!gh auth git-credential"
    -e "GIT_CONFIG_KEY_2=user.name" -e "GIT_CONFIG_VALUE_2=$bot_name"
    -e "GIT_CONFIG_KEY_3=user.email" -e "GIT_CONFIG_VALUE_3=$bot_email"
    -e GIT_TERMINAL_PROMPT=0)
  while IFS='=' read -r var _; do
    case "$var" in PI_ENSEMBLE_IMAGE | PI_ENSEMBLE_HOST_ALIASES) ;; PI_ENSEMBLE_*) sb_envs+=(-e "$var") ;; esac
  done < <(env)

  # A fresh workspace, at the tip the product checkout last fetched for the
  # branch. What an earlier run left behind goes with the old one. Only a
  # directory this command made is ever removed.
  base=$(git -C "$src" rev-parse -q --verify "refs/remotes/origin/$branch^{commit}") ||
    { echo "$cmd: $src has no origin/$branch, not starting" >&2; exit 78; }
  url=$(git -C "$src" config --get remote.origin.url)
  works=$(dirname "$sb_repo")
  if [ -e "$works" ] && [ ! -f "$works/.tehas-workspaces" ]; then
    echo "$cmd: $works exists and was not made by tehas, not touching it" >&2
    exit 78
  fi
  if ! { mkdir -p "$works" && : >"$works/.tehas-workspaces" && rm -rf "$sb_repo" &&
    git init -q -b "$branch" "$sb_repo" &&
    git -C "$sb_repo" remote add origin "$url" &&
    git -C "$sb_repo" fetch -q --no-tags "$src" '+refs/remotes/origin/*:refs/remotes/origin/*' &&
    git -C "$sb_repo" symbolic-ref refs/remotes/origin/HEAD "refs/remotes/origin/$branch" &&
    git -C "$sb_repo" checkout -q -B "$branch" "$base" &&
    git -C "$sb_repo" branch -q --set-upstream-to="origin/$branch" "$branch"; }; then
    echo "$cmd: could not make the workspace $sb_repo, not starting" >&2
    exit 78
  fi
}

sandbox_pi() {
  local name=$1 log=$2 fifo
  shift 2
  fifo=$(mktemp -u)
  mkfifo "$fifo"
  exec 3<>"$fifo"
  rm "$fifo"
  docker run --rm -i --name "$name" \
    --cpus "$TEHAS_WORK_CPUS" --memory "$TEHAS_WORK_MEMORY" \
    -w "$sb_repo" -v "$sb_repo:$sb_repo" \
    -v "$HOME/.vipune:/home/vscode/.vipune" \
    -v "$sb_agent/ensemble-runs:/home/vscode/.pi/agent/ensemble-runs" \
    -v "$sb_agent/sessions:/home/vscode/.pi/agent/sessions" \
    -v "$sb_agent/models.json:/home/vscode/.pi/agent/models.json:ro" \
    -v "$sb_agent/mcp.json:/home/vscode/.pi/agent/mcp.json:ro" \
    -v pi-ensemble-cache:/home/vscode/.cache \
    -v pi-ensemble-bun-cache:/home/vscode/.bun \
    "${sb_envs[@]}" "${sb_hosts[@]}" \
    "$sb_image" pi --mode rpc --approve "$@" <&3 >"$log" 2>"$log.err" &
  sb_pid=$!
}
