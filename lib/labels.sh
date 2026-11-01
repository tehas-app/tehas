# shellcheck shell=bash
# Who applied a label: the rules and the usage are console/src/label.rs
# (AGENTS.md rule 5).
#
#   tehas_label_actor <label>             stdin: an issue's events
#   tehas_allowed_labeler <login>         0 when <login> is one of
#                                         TEHAS_ALLOWED_LABELERS, for callers
#                                         in tight paths and with no console
#   tehas_label_allowed <issue> <label>   0 yes, 1 no, 2 events unread
#
# Sourcing this file does nothing.

tehas_labels_console=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)/libexec/console
_tehas_label() {
  TEHAS_REPO=${TEHAS_REPO:-} TEHAS_ALLOWED_LABELERS=${TEHAS_ALLOWED_LABELERS:-} \
    "${TEHAS_CONSOLE_CMD:-${TEHAS_CONSOLE:-$tehas_labels_console}}" label "$@"
}
tehas_label_actor() { _tehas_label actor "$1"; }
tehas_label_allowed() { _tehas_label allowed "$1" "$2"; }

tehas_allowed_labeler() {
  # lib/config.sh checks the setting for logins separated by single spaces, so
  # padding the list and the name with one space each matches whole logins only.
  case "${1:+ ${TEHAS_ALLOWED_LABELERS:-} }" in
    *" $1 "*) return 0 ;;
    *) return 1 ;;
  esac
}
