# shellcheck shell=bash
# Who applied a label. Authorisation is the account behind the label event, not
# that the label is present (AGENTS.md rule 5), so every command that lets a
# label decide something reads the same events the same way. Sourced by
# the dispatcher (tehas:ready, tehas:plan, tehas:refine, tehas:urgent,
# tehas:next, tehas:blocked), and by tehas work and tehas plan (whether the
# label that started them approved an issue's images).
#
#   tehas_label_actor <label>       stdin: one issue's events, as
#                                   gh api repos/<repo>/issues/<n>/events?per_page=100
#                                   --paginate answers them (one JSON array per
#                                   page). stdout: "<login>\t<time>" for the
#                                   labeled event carrying <label> with the
#                                   latest time - newest by its own stamp, not
#                                   by the order GitHub handed the pages back,
#                                   except that events sharing one stamp keep
#                                   that page order and an event with no stamp
#                                   is older than any stamped one - or an empty
#                                   pair when no event does: an event that names
#                                   no account or no time leaves that field
#                                   empty. Fails when the events are not what
#                                   this expects, which is never the same as
#                                   having no event.
#   tehas_allowed_labeler <login>   0 when <login> is one of
#                                   TEHAS_ALLOWED_LABELERS. An empty name is
#                                   nobody and is never allowed.
#   tehas_label_allowed <issue> <label>
#                                   asks GitHub for the issue's events in
#                                   TEHAS_REPO: 0 when the account that applied
#                                   <label> most recently is allowed, 1 when it
#                                   is not or no event applies it, 2 when the
#                                   events cannot be read.
#
# Which issue to ask about, and what to do when its events cannot be read, is
# the caller's. Sourcing this file does nothing.

tehas_label_actor() {
  # A tab between the two fields: a login holds no tab, and either may be empty.
  jq -rs --arg l "$1" '
    if any(.[]; type != "array") then error("not a list of events") else . end
    | (add // [])
    | map(select(.event == "labeled" and .label.name == $l))
    # GitHub documents no order for these events, so the stamp decides: equal
    # stamps keep the order the pages arrived in, a missing stamp sorts first.
    | sort_by(.created_at)
    | last
    | "\(.actor.login // "")\t\(.created_at // "")"'
}

tehas_allowed_labeler() {
  # lib/config.sh checks the setting for logins separated by single spaces, so
  # padding the list and the name with one space each matches whole logins only.
  case "${1:+ ${TEHAS_ALLOWED_LABELERS:-} }" in
    *" $1 "*) return 0 ;;
    *) return 1 ;;
  esac
}

tehas_label_allowed() { # <issue> <label>
  local events line
  events=$(gh api "repos/$TEHAS_REPO/issues/$1/events?per_page=100" --paginate 2>/dev/null) || return 2
  line=$(tehas_label_actor "$2" <<<"$events") || return 2
  tehas_allowed_labeler "${line%%$'\t'*}"
}
