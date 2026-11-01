# shellcheck shell=bash
# shellcheck disable=SC2154  # here is the sourcing command's
# The bash of what a sandbox container is given, and the tree it works in.
# Sourced by the commands that start one in bash: tehas work (a cycle) and tehas
# plan (a planning turn), and by tehas dev for the check of Pi's files.
# The docker options themselves are made in one
# place, console/core/src/sandbox.rs, and reach bash through tehas-cycle
# sandbox, which stands beside the command as "$here/tehas-cycle". The goldens
# of test/sandbox_argv_test.sh pin the result word by word.
#
#   tehas_sandbox_ok                     Pi's files are there and the model
#                                        tokens set; else 78, said
#   tehas_sandbox_limits <command> <what>
#                                        the container's limits and security
#                                        options in the caller's
#                                        TEHAS_SLICE_LIMITS and sec; else 78, said
#   tehas_sandbox_mounts <kind> <tree> <host> <state> <runs>
#                                        make what the -v options name and the
#                                        container's own models.json, and print
#                                        the options NUL separated
#   tehas_sandbox_retry <host>           the provider retry a container's Pi runs
#                                        with, as one line
#   tehas_sandbox_seed <state> [<index dir>]
#                                        give <state> the host's code index
#   tehas_index_dir                      where the host keeps the index
#   tehas_git_github_host                the github.com host git authenticates to
#   tehas_git_credential_helper_key      the config key of that host's helper
#   tehas_sandbox_helper_command         the helper command a container's git runs
#   tehas_sandbox_clone <src> <tree> <branch>
#                                        make <tree>, a clone of <src>
#
# Source lib/config.sh and call tehas_load first, and set here to the directory
# of the command: the settings are read from the environment it leaves.

# Where the host keeps the code index it gives to containers: the one directory
# the indexer writes and every container is given a copy of. No container is
# given it itself. Derived from TEHAS_HOME, not a setting.
tehas_index_dir() { printf '%s\n' "$TEHAS_HOME/state/index"; }

# The github.com host, and the git config key that names its credential
# helper. Two things read that key: the probe of tehas setup, which asks the
# host's git whether it has a helper, and the environment-level config below,
# which is what a container's git is given. A change here (another host,
# another subsection spelling) moves both; no other github.com of the platform
# is named by these.
tehas_git_github_host() { printf '%s\n' github.com; }
tehas_git_credential_helper_key() {
  local h
  h=$(tehas_git_github_host)
  printf '%s\n' "credential.https://$h.helper"
}

# The command a container's git runs to ask gh for a credential, read out of
# GH_TOKEN. The host's gh is asked to write the same one; the test of that
# write and the container's entry read it here.
tehas_sandbox_helper_command() { printf '%s\n' '!gh auth git-credential'; }

# Asked before a command calls Docker or git or writes anything. A command with
# no tehas-cycle beside it goes on to tehas_sandbox_limits, which refuses it.
tehas_sandbox_ok() {
  "$here/tehas-cycle" sandbox ok
  [ "$?" != 78 ]
}

# <command> leads the line a refusal says, <what> names what does not start. The
# program said why when it answers 78. What it hands out is checked here all the
# same, so a list that drifted starts nothing: a short one, another parent, or
# security options that are not the ten of "What a container may not do".
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_sandbox_limits() { # <command> <what>
  local words rc
  mapfile -d '' -t words < <("$here/tehas-cycle" sandbox limits "$1" "$2")
  wait "$!"
  rc=$?
  [ "$rc" != 78 ] || return 78
  TEHAS_SLICE_LIMITS=("${words[@]:0:14}")
  sec=("${words[@]:14:10}")
  if [ "$rc" != 0 ] || [ "${#TEHAS_SLICE_LIMITS[@]}" != 14 ] || [ "${TEHAS_SLICE_LIMITS[1]}" != tehas.slice ] ||
    [ "${TEHAS_SLICE_LIMITS[12]}" != --cpu-shares ]; then
    echo "$1: the container limits could not be made, not starting" >&2
    return 78
  fi
  case "${#sec[@]} ${sec[*]}" in
    "10 --cap-drop ALL --cap-add CHOWN --cap-add FOWNER --security-opt no-new-privileges:true --tmpfs /tmp:rw,exec,nosuid,nodev,size="[1-9]*m,mode=1777) ;;
    *)
      echo "$1: tehas_sandbox_security gave ${#sec[@]} words, wants 10: not starting $2" >&2
      return 78
      ;;
  esac
}

# Writes <state>/models.json and <state>/endpoints too. The options come last,
# so a run that could not write those is given none.
tehas_sandbox_mounts() { "$here/tehas-cycle" sandbox mounts "$@"; }

# The provider and agent retry and timeout a container's Pi runs with: the host's
# settings.json that a container is mounted, under the product's own
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
    cp -a "$f" "$state/cache/" || { rm -rf "$state/cache"; mkdir -p "$state/cache"; return 1; }
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
