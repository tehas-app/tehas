# shellcheck shell=bash
# Tehas instance settings, for login shells (env.sh), install.sh and updates.
# <instance dir>/instance.env is KEY=value lines, sourced as written: keep it
# writable by the tehas user only, and put no secrets in it.
#
#   tehas_load  find the instance, load and check its settings. Returns 78 with
#               one line on stderr when it cannot. It never exits, because
#               login shells source it too.
#   tehas_require_instance_settings
#               exit 78 unless every setting an instance has to set itself is
#               set and valid. An update asks it of the tree it is about to run.
#
# The rules are the console's (console/core/src/settings/). Loading runs no git
# command, makes no network call and writes nothing.

tehas_err() { echo "tehas: $*" >&2; }

# The environment, else the one-line file ~/.config/tehas/instance-dir, else ~/.config/tehas.
tehas_instance_dir() {
  local dir=${TEHAS_INSTANCE_DIR:-} pointer=$HOME/.config/tehas/instance-dir
  [ -n "$dir" ] || [ ! -r "$pointer" ] || read -r dir <"$pointer" || true
  printf '%s\n' "${dir:-$HOME/.config/tehas}"
}

# TEHAS_SETTINGS_CMD, which a build or a test names, else the console beside this file, else the home's.
_tehas_settings_console() {
  local beside=${BASH_SOURCE[0]%/*}/../libexec/console
  [ -x "$beside" ] || beside=$TEHAS_HOME/libexec/console
  printf '%s\n' "${TEHAS_SETTINGS_CMD:-$beside}"
}

# The console sees every name of the families, to drop or check. This shell
# changes only by the lines it prints: one that cannot run leaves it as it was.
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

tehas_require_instance_settings() {
  (export "${!TEHAS_@}" "${!PI_ENSEMBLE_@}" "${!PI_RUKAS_@}"; "$(_tehas_settings_console)" settings require "$EUID" "$BASH" </dev/null) || exit 78
}

# tehas update from before v0.21.0 sources this file from the checkout it pulled
# with no console here and one at home with no settings verb. Without tehas_load
# it leaves the check to install.sh. Drop this when no host runs such an update.
case ${BASH_EXECUTION_STRING:-} in
  *'install.sh will check the settings itself'*)
    [ -x "${BASH_SOURCE[0]%/*}/../libexec/console" ] || [ -n "${TEHAS_SETTINGS_CMD:-}" ] || unset -f tehas_load
    ;;
esac
