# shellcheck shell=bash
# The free kilobytes a command holds work on: console disk free. 1 and nothing
# printed is no figure. Never sourced by lib/config.sh: it calls df and docker.
tehas_disk_console=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/libexec/console
tehas_free_kb() { # <path> [<seconds>]
  "${TEHAS_CONSOLE_CMD:-$tehas_disk_console}" disk free "${1:-}" "${2:-${TEHAS_DISK_WAIT:-}}" 2>/dev/null
}
