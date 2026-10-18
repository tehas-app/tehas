# shellcheck shell=bash
# Who applied a label. Authorisation is the account behind the label event, not
# that the label is present (AGENTS.md rule 5), so every command that lets a
# label decide something reads the same events the same way. Sourced by
# libexec/dispatch (tehas:ready, tehas:plan, tehas:urgent, tehas:next,
# tehas:blocked), by tehasbot protolab (tehas:prototype), and by tehas work and
# tehas plan (whether the label that started them approved an issue's images).
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
#   tehas_label_state <label>       stdin: one issue's events, as above.
#                                   stdout: "<event>\t<login>\t<time>" for the
#                                   labeled or unlabeled event of <label> with
#                                   the latest stamp, ordered as
#                                   tehas_label_actor orders them, except that a
#                                   removal sharing a stamp with an application
#                                   comes after it. Empty fields when no event
#                                   names the label. Fails as tehas_label_actor
#                                   does.
#   tehas_allowed_labeler <login>   0 when <login> is one of
#                                   TEHAS_ALLOWED_LABELERS. An empty name is
#                                   nobody and is never allowed.
#   tehas_label_denied <issue> <label> [<account>]
#                                   one line naming the issue, the label and the
#                                   account that applied it. The caller logs it.
#   tehas_label_allowed <issue> <label>
#                                   asks GitHub for the issue's events in
#                                   TEHAS_REPO: 0 when the account that applied
#                                   <label> most recently is allowed, 1 when it
#                                   is not or no event applies it, 2 when the
#                                   events cannot be read.
#   tehas_pause_holds               stdin: one issue's events, as above, for the
#                                   control issue. The pause label works one way:
#                                   anyone who can label may stop the factory,
#                                   only an allowed account may start it again.
#                                   0 when the tehas:pause event with the latest
#                                   stamp holds the host paused: a removal by an
#                                   account outside TEHAS_ALLOWED_LABELERS or by
#                                   an event that names no account, or an
#                                   application no event has taken back - the
#                                   label is gone from the issue all the same,
#                                   which means the label object was deleted or
#                                   renamed, and no account was asked to lift it.
#                                   stdout is "<event>\t<account>", the account
#                                   empty when the event names none. 1 when
#                                   nothing the events say holds the host paused:
#                                   no tehas:pause event, or its last removal was
#                                   by an allowed account. 2 when the events are
#                                   not what this expects, which is never the
#                                   same as having no event. Whether the label is
#                                   on the issue now is not this function's to
#                                   see: the caller reads the issue.
#
# Which issue to ask about, and what to do when its events cannot be read, is
# the caller's: a poll that cannot ask GitHub ends, the Protolab leaves a
# ticket's page as it is. Sourcing this file does nothing.

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

tehas_label_state() {
  jq -rs --arg l "$1" '
    if any(.[]; type != "array") then error("not a list of events") else . end
    | (add // [])
    | map(select((.event == "labeled" or .event == "unlabeled") and .label.name == $l))
    # Of an application and a removal in one second, the removal is read last:
    # the label then counts for nothing.
    | sort_by([.created_at // "", if .event == "unlabeled" then 1 else 0 end])
    | last // {}
    | "\(.event // "")\t\(.actor.login // "")\t\(.created_at // "")"'
}

tehas_allowed_labeler() {
  # lib/config.sh checks the setting for logins separated by single spaces, so
  # padding the list and the name with one space each matches whole logins only.
  case "${1:+ ${TEHAS_ALLOWED_LABELERS:-} }" in
    *" $1 "*) return 0 ;;
    *) return 1 ;;
  esac
}

tehas_label_denied() {
  printf '#%s: %s was applied by %s, not an allowed account' "$1" "$2" "'${3:-unknown}'"
}

tehas_label_allowed() { # <issue> <label>
  local events line
  events=$(gh api "repos/$TEHAS_REPO/issues/$1/events?per_page=100" --paginate 2>/dev/null) || return 2
  line=$(tehas_label_actor "$2" <<<"$events") || return 2
  tehas_allowed_labeler "${line%%$'\t'*}"
}

# tehas_pause_holds: as tehas_label_actor reads the events, but for both kinds
# of pause event: what stopped the factory, and what took the stop away.
tehas_pause_holds() {
  local line event by
  line=$(jq -rs '
    if any(.[]; type != "array") then error("not a list of events") else . end
    | (add // [])
    | map(select((.event == "labeled" or .event == "unlabeled") and .label.name == "tehas:pause"))
    # GitHub documents no order for these events, so the stamp decides: a missing
    # stamp sorts first. Two events sharing one stamp are put in the order that
    # holds the host paused, removal before application, so that a pause applied
    # and lifted inside one second is read as still applied.
    | sort_by([.created_at // "", if .event == "unlabeled" then 0 else 1 end])
    | last // {}
    | "\(.event // "")\t\(.actor.login // "")"' 2>/dev/null) || return 2
  event=${line%%$'\t'*}
  by=${line#*$'\t'}
  case "$event" in
    # The label came off: who took it off decides.
    unlabeled) tehas_allowed_labeler "$by" && return 1 ;;
    # Applied, and nothing has taken it off. That is the one state where a pause
    # would else be lifted by no account at all, so it holds the host.
    labeled) ;;
    *) return 1 ;;
  esac
  printf '%s\t%s\n' "$event" "$by"
}
