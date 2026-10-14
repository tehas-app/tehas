# shellcheck shell=bash
# Which cycles are running, where a parked cycle's question is on record, and
# what a number on GitHub is. Sourced by the dispatcher, by tehasbot cycle and
# by tehas index.
#
# tehasbot cycle holds $TEHAS_HOME/state/cycles/<issue>.lock for as long as it
# lives. A lock nobody holds is a cycle that ended, however it ended, so a
# cycle that died is never counted.
#
#   tehas_cycle_running <issue>   0 if that issue's cycle holds its lock
#   tehas_cycles_running          the issues with a running cycle, one per line
#   park_id <issue>               the path of the file that holds the id of
#                                 the park question this host posted
#
# Needs TEHAS_HOME. Reads only: it takes each lock for an instant and lets go.

tehas_cycle_running() {
  local lock=$TEHAS_HOME/state/cycles/$1.lock
  [ -e "$lock" ] && ! flock -n "$lock" true
}

tehas_cycles_running() {
  local lock n
  for lock in "$TEHAS_HOME"/state/cycles/*.lock; do
    [ -e "$lock" ] || continue
    n=${lock##*/}
    n=${n%.lock}
    if tehas_cycle_running "$n"; then printf '%s\n' "$n"; fi
  done
  return 0
}

# An agent in a container holds the bot's token: it can post a comment with a
# marker, edit the bot's comments and apply labels. It cannot write here, so
# this file is what makes a comment the factory's own question. One line, the
# comment's number. tehasbot cycle writes it, the dispatcher reads it.
park_id() { printf '%s/state/park-%s.id\n' "$TEHAS_HOME" "$1"; }

# The factory works on open issues only: a closed issue is never taken, and a
# pull request is never taken, open, merged or closed. What GitHub answers for
# a number decides it, not which labels that number carries.
#
#   tehas_work_kind <the answer for that number>
#     0  the answer is an open issue, which the factory may take
#     1  it is anything else, with the reason in $tehas_work_why: pr (a pull
#        request), closed, or unreadable (no answer, or one that is not the
#        answer for a single issue). A number is never taken on a guess.
tehas_work_why=""
tehas_work_kind() {
  local kind
  kind=$(jq -r 'if type == "object" and ((.pull_request // null) != null) then "pr"
                elif type == "object" and ((.state // "") | ascii_downcase) == "open" then "open"
                elif type == "object" and ((.state // "") | ascii_downcase) == "closed" then "closed"
                else "unreadable" end' <<<"${1:-}" 2>/dev/null) || kind=""
  case "$kind" in
    open) tehas_work_why=open ;;
    pr | closed) tehas_work_why=$kind ;;
    *) tehas_work_why=unreadable; return 1 ;;
  esac
  [ "$tehas_work_why" = open ]
}
