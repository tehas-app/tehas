# shellcheck shell=bash
# Tehas instance settings. Sourced by every tehas command and by env.sh.
#
# An instance is one host serving one product repository. Its settings are in
# <instance dir>/instance.env, a shell file of KEY=value lines written by the
# operator. It is sourced as written: treat it like a script, keep it writable
# by the tehas user only, and put no secrets in it.
#
#   tehas_load           find the instance, load and check its settings.
#                        Returns 78 with one line on stderr when it cannot. It
#                        never exits, because login shells source it too.
#   tehas_require NAME.. exit 78 unless each named setting is set and valid.
#   tehas_require_instance_settings
#                        exit 78 unless every setting an instance has to set for
#                        itself is set and valid. One list, so no command that
#                        checks it before changing anything has its own copy.
#   tehas_checkout       check the product checkout is a clone of TEHAS_REPO;
#                        returns 78 if not.
#   tehas_branch         print the branch work lands on; returns 78 if unknown.
#   tehas_models         print "provider model" for every model the agents use.
#
# A command starts with:
#   here=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
#   . "$here/../lib/config.sh"
#   tehas_load || exit 78
#   tehas_require TEHAS_REPO ...   (or tehas_require_instance_settings)
#
# The rules are the console's: console/core/src/settings/rules.rs holds what
# each setting may hold and load.rs what a load does. Loading runs no git
# command, makes no network call and writes nothing.

tehas_err() { echo "tehas: $*" >&2; }

# Where the instance's settings are: the environment, else the one-line file
# ~/.config/tehas/instance-dir, else ~/.config/tehas.
tehas_instance_dir() {
  local dir=${TEHAS_INSTANCE_DIR:-} pointer=$HOME/.config/tehas/instance-dir
  [ -n "$dir" ] || [ ! -r "$pointer" ] || read -r dir <"$pointer" || true
  printf '%s\n' "${dir:-$HOME/.config/tehas}"
}

# What tehas_require reads: each setting that holds no value of its rule, and
# why, with _tehas_loaded to say a load made it. Declared once, so a second
# source of this file keeps what a load said.
declare -gA _tehas_bad

# The console that reads the settings: TEHAS_SETTINGS_CMD, which a build or a
# test names, else the one beside this file, else the home's.
_tehas_settings_console() {
  local beside=${BASH_SOURCE[0]%/*}/../libexec/console
  if [ -n "${TEHAS_SETTINGS_CMD:-}" ]; then
    printf '%s\n' "$TEHAS_SETTINGS_CMD"
  elif [ -x "$beside" ]; then
    printf '%s\n' "$beside"
  else
    printf '%s\n' "$TEHAS_HOME/libexec/console"
  fi
}

# The subshell exports the families so the console sees every name it may have
# to drop or check. This shell's names change only by the lines the console prints:
# one that cannot run, or says something else, leaves a login shell as it was.
tehas_load() {
  local _tehas_console _tehas_lines _tehas_rc=0
  : "${TEHAS_HOME:=$HOME/tehas}"
  export TEHAS_HOME
  _tehas_console=$(_tehas_settings_console)
  [ -x "$_tehas_console" ] || { tehas_err "no console at $_tehas_console to load the settings: run install.sh"; return 78; }
  _tehas_lines=$(export "${!TEHAS_@}" "${!PI_ENSEMBLE_@}" "${!PI_RUKAS_@}"; "$_tehas_console" settings load "$EUID" "$BASH" </dev/null) || _tehas_rc=$?
  case "$_tehas_rc" in
    0 | 78) eval "$_tehas_lines"; return "$_tehas_rc" ;;
  esac
  tehas_err "the console at $_tehas_console cannot load the settings (status $_tehas_rc): run install.sh"
  return 78
}

tehas_require() {
  local name file=${TEHAS_INSTANCE_DIR:-?}/instance.env
  [ -n "${_tehas_loaded:-}" ] || { tehas_err "no settings are loaded here, so none can be required"; exit 78; }
  for name in "$@"; do
    # Only spaces counts as not set.
    [ -n "${!name:-}" ] && [ -n "${!name// /}" ] || { tehas_err "$name is not set (see $file)"; exit 78; }
    [ -z "${_tehas_bad[$name]:-}" ] || { tehas_err "$name is not valid: ${_tehas_bad[$name]} (see $file)"; exit 78; }
  done
}

# The settings an instance has to set itself, because nothing here can give them
# a sensible default: install.sh, tehas-dispatch and tehas update each check this
# one list before they call GitHub, Docker, git or write a file.
tehas_require_instance_settings() {
  tehas_require TEHAS_REPO TEHAS_CONTROL_ISSUE TEHAS_ALLOWED_LABELERS \
    PI_ENSEMBLE_SUBAGENT_MODEL PI_ENSEMBLE_SUBAGENT_PROVIDER
}

# Every provider and model pair the agents use, one "provider model" line
# each, without repeats: the subagent default first, then each role's own.
tehas_models() {
  local role m p
  {
    printf '%s %s\n' "${PI_ENSEMBLE_SUBAGENT_PROVIDER:-}" "${PI_ENSEMBLE_SUBAGENT_MODEL:-}"
    for role in DEVELOPER OPS EXPLORE ADVERSARIAL_DEVELOPER CODE_REVIEW_SPECIALIST; do
      m=PI_ENSEMBLE_MODEL_$role p=PI_ENSEMBLE_PROVIDER_$role
      [ -z "${!m:-}" ] || printf '%s %s\n' "${!p:-}" "${!m}"
    done
  } | awk '$1 != "" && $2 != "" && !seen[$0]++'
}

# The product checkout must exist and be a clone of TEHAS_REPO: issues are
# polled from one and the work happens in the other, so they have to be the
# same repository. Returns 78 otherwise. Reads the checkout's configuration
# only; nothing is fetched.
tehas_checkout() {
  local dir=${TEHAS_WORK_REPO:-} url
  if [ -z "$dir" ] || [ ! -d "$dir/.git" ]; then
    tehas_err "no product checkout at ${dir:-(unset)}: clone $TEHAS_REPO there, or set TEHAS_WORK_REPO in ${TEHAS_INSTANCE_DIR:-?}/instance.env"
    return 78
  fi
  url=$(git -C "$dir" config --get remote.origin.url 2>/dev/null) || url=""
  url=${url%/}
  url=${url%.git}
  case "$url" in
    */"$TEHAS_REPO" | *:"$TEHAS_REPO") return 0 ;;
  esac
  tehas_err "$dir is a checkout of ${url:-nothing}, not of $TEHAS_REPO"
  return 78
}

# The branch cycles start from and pull requests target. Read locally: the
# checkout's origin/HEAD is set by install.sh, never looked up here.
tehas_branch() {
  local b=""
  [ -z "${TEHAS_BRANCH:-}" ] || { printf '%s\n' "$TEHAS_BRANCH"; return 0; }
  b=$(git -C "${TEHAS_WORK_REPO:-/nonexistent}" symbolic-ref -q --short refs/remotes/origin/HEAD 2>/dev/null) || b=""
  b=${b#origin/}
  [ -n "$b" ] || {
    tehas_err "the default branch of ${TEHAS_WORK_REPO:-the product checkout} is unknown: run install.sh, or set TEHAS_BRANCH in ${TEHAS_INSTANCE_DIR:-?}/instance.env"
    return 78
  }
  printf '%s\n' "$b"
}
