# shellcheck shell=bash
# shellcheck disable=SC2154  # here is the sourcing command's
# The bash of what a sandbox container is given.
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
#   tehas_index_dir                      where the host keeps the index
#   tehas_git_github_host                the github.com host git authenticates to
#   tehas_git_credential_helper_key      the config key of that host's helper
#
# Source lib/config.sh and call tehas_load first, and set here to the directory
# of the command: the settings are read from the environment it leaves.

# shellcheck source=lib/models.sh
. "$(dirname "${BASH_SOURCE[0]}")/models.sh"

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

# models.json is the container's own, <state>/models.json: the host's with each
# provider on the endpoint picked now (lib/models.sh). <state>/endpoints says
# which providers are on their backup. The options are held until that is
# written, so a run that could not write it is given none.
tehas_sandbox_mounts() { # <kind> <tree> <host> <state> <runs>
  local opts
  mapfile -d '' -t opts < <("$here/tehas-cycle" sandbox mounts "$@")
  wait "$!" && tehas_models_resolve "$4/models.json" >"$4/endpoints" && printf '%s\0' "${opts[@]}"
}
