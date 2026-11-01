# shellcheck shell=bash
# The commands' way to tehas.slice, the systemd slice every sandbox container
# runs in. The rules and the reads are `console slice`: console/src/slice.rs
# says what each verb prints. Sourced by the dispatcher and tehas improve. Never
# by lib/config.sh or a login shell: it calls systemctl and docker, and loading
# settings must not.
#
# A command that starts a container asks `tehas-cycle sandbox limits`, which
# keeps the same rules. With no console there is no slice: not ok, and no room.

tehas_slice_console=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/libexec/console
tehas_slice_none="no console beside this command at $tehas_slice_console, run install.sh"

# What ok sets is not exported, so each verb is handed it here.
_tehas_slice() {
  TEHAS_SLICE_MAX=${TEHAS_SLICE_MAX:-} TEHAS_SLICE_ACTIVE=${TEHAS_SLICE_ACTIVE:-} \
    TEHAS_SLICE_CEILING_MB=${TEHAS_SLICE_CEILING_MB:-} \
    "${TEHAS_CONSOLE_CMD:-$tehas_slice_console}" slice "$@" 2>/dev/null
}

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_slice_ok() {
  local out rc=0
  TEHAS_SLICE_WHY=$tehas_slice_none TEHAS_SLICE_MAX="" TEHAS_SLICE_ACTIVE="" TEHAS_SLICE_CEILING_MB="" TEHAS_SLICE_CPUS=""
  out=$(_tehas_slice ok) || rc=1
  eval "$out"
  return "$rc"
}

tehas_slice_room() {
  _tehas_slice room "$@" || echo "0 0 ${TEHAS_SLICE_MAX:-0} 0 unreadable $tehas_slice_none"
}

# shellcheck disable=SC2034  # the caller reads what this sets
tehas_slice_pressure() {
  local out
  TEHAS_SLICE_WHY=""
  if out=$(_tehas_slice pressure); then
    echo "$out"
    return 0
  fi
  TEHAS_SLICE_WHY=${out:-$tehas_slice_none}
  return 1
}
