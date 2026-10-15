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
#   tehas_rukas_commit            the pi-rukas commit a cycle started now runs
#   tehas_rukas_proven <commit>   0 if a cycle on this host ended well on it
#   tehas_names_issue             print a jq definition, `mine`: whether a pull
#                                 request of gh pr list names the issue $n
#
# Needs TEHAS_HOME. Reads only: it takes each lock for an instant and lets go.
# tehas_rukas_commit asks Docker about PI_ENSEMBLE_IMAGE.

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

# What a cycle runs is the sandbox image, so its label tehas.rukas says the
# commit, also for an image put back for a rollback. An image built before the
# label has the commit versions.env names. Nothing when neither says.
tehas_rukas_commit() {
  local c
  c=$(docker image inspect --format '{{index .Config.Labels "tehas.rukas"}}' "${PI_ENSEMBLE_IMAGE:-}" 2>/dev/null) || c=""
  [[ $c =~ ^[0-9a-f]{7,40}$ ]] ||
    c=$(sed -n 's/^PI_RUKAS_COMMIT=\([0-9a-f]\{7,40\}\)$/\1/p' "$TEHAS_HOME/versions.env" 2>/dev/null | head -n 1)
  printf '%s\n' "$c"
}

# state/rukas-proven: one commit a line, each one a cycle ended well on.
tehas_rukas_proven() { [ -n "$1" ] && grep -qxF -e "$1" "$TEHAS_HOME/state/rukas-proven" 2>/dev/null; }

# A pull request names an issue when its head is the issue's branch as pi-rukas
# names it, feature/issue-<n>-..., or its body closes #<n>. tehasbot cycle and
# the dispatcher must agree on which pull request is a cycle's.
tehas_names_issue() {
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def mine: ((.headRefName // "") | tostring | (. == "feature/issue-\($n)" or startswith("feature/issue-\($n)-")))
  or ((.body // "") | tostring | test("(?i)\\b(close[sd]?|fix(e[sd])?|resolve[sd]?):?\\s+#\($n)([^0-9]|$)"));'
}
