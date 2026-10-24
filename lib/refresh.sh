# shellcheck shell=bash
# Refreshes: the factory's own pull requests that conflict with the branch,
# and what a refresh of one rests on. Sourced by tehasbot refresh
# (libexec/refresh), which finds them, and by the dispatcher, which checks
# each again before it starts one. Both read a pull request through the same
# query and the same rule, so they cannot disagree on which one qualifies.
#
#   tehas_refresh_dir             state/refresh, where the requests are
#   tehas_refresh_query           print the GraphQL query of the open pull
#                                 requests of the repository, with what the
#                                 rule reads of each and of the issue it closes
#   tehas_refresh_pr_query        print the GraphQL query of one pull request,
#                                 the same fields, as variable $number
#   tehas_refresh_rule            print a jq definition, `refresh_why`: of one
#                                 pull request node, why it is not one the
#                                 factory refreshes, "" when it is. Needs $bot
#                                 (an app's [bot] suffix is dropped: GraphQL
#                                 names an app without it),
#                                 $repo and $control as jq arguments. The
#                                 mergeable state is the caller's to read
#   tehas_refresh_count <record> <base>
#                                 the refreshes the approval record counts
#                                 since the approval <base> (JSON, from
#                                 tehas_fixer_base): its refreshes when it is
#                                 a refresh resting on that approval, else 0
#   tehas_refresh_parked <issue> <base>
#                                 0 when a refresh of the issue parked since
#                                 the approval <base>, by its history lines
#   tehas_refresh_continues       0 when the sandbox image's pi-rukas takes
#                                 --continue-pr: its --capabilities answer has
#                                 features.continuePr. Asked once per command
#
# Needs TEHAS_HOME, lib/fixer.sh for the history and lib/headless.sh for the
# answer. Sourcing it does nothing.

tehas_refresh_dir() { printf '%s/state/refresh\n' "$TEHAS_HOME"; }

# The fields of a pull request the rule reads.
_tehas_refresh_fields='number url isDraft isCrossRepository headRefOid baseRefName baseRefOid mergeable
        author { login } headRepository { nameWithOwner } labels(first: 50) { nodes { name } }
        closingIssuesReferences(first: 5) { totalCount nodes { number state labels(first: 50) { nodes { name } }
          closedByPullRequestsReferences(first: 10) { nodes { number state } } } }'

tehas_refresh_query() {
  # shellcheck disable=SC2016  # $owner and $name are GraphQL variables
  printf '%s\n' 'query($owner: String!, $name: String!) { repository(owner: $owner, name: $name) {
      pullRequests(states: OPEN, first: 100) { nodes { '"$_tehas_refresh_fields"' } } } }'
}

tehas_refresh_pr_query() {
  # shellcheck disable=SC2016  # $owner, $name and $number are GraphQL variables
  printf '%s\n' 'query($owner: String!, $name: String!, $number: Int!) { repository(owner: $owner, name: $name) {
      pullRequest(number: $number) { state '"$_tehas_refresh_fields"' } } }'
}

# Only the bot's own pull request of one open issue that waits for a merge or
# was handed off (needs-human-attention) and has no cycle running, from a
# branch of the repository and no draft, and the only open one that closes
# that issue. A shape it does not know is a reason too.
tehas_refresh_rule() {
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def refresh_why:
    def names: [(.labels.nodes // [])[]? | .name? | strings];
    if type != "object" or (.number | type) != "number" then "it cannot be read"
    elif (.author.login // "") != ($bot | sub("\\[bot\\]$"; "")) then "it is not the bot'"'"'s"
    elif .isCrossRepository != false or ((.headRepository.nameWithOwner // "") | ascii_downcase) != ($repo | ascii_downcase) then "its branch is not in \($repo)"
    elif .isDraft != false then "it is a draft"
    elif (.closingIssuesReferences.totalCount // 0) != 1 or ((.closingIssuesReferences.nodes // []) | length) != 1 then "it does not close exactly one issue"
    else (.number) as $pr | .closingIssuesReferences.nodes[0]
      | if (.number | type) != "number" then "the issue it closes cannot be read"
        elif .number == $control then "it closes the control issue"
        elif .state != "OPEN" then "#\(.number) is not open"
        elif (names | index("tehas:awaiting-merge")) == null and (names | index("needs-human-attention")) == null
          then "#\(.number) carries neither tehas:awaiting-merge nor needs-human-attention"
        elif (names | index("tehas:running")) != null then "#\(.number) carries tehas:running"
        elif [(.closedByPullRequestsReferences.nodes // [])[]? | select(.state? == "OPEN") | .number] != [$pr] then "it is not the one open pull request that closes #\(.number)"
        else "" end
    end;'
}

tehas_refresh_count() { # <record> <base>
  jq -r --argjson base "$2" 'if .how == "refresh" and (.rests_on | {at, by}) == ($base | {at, by})
      then (.refreshes | if type == "number" and . >= 1 and . == floor then . else error("no count") end)
      else 0 end' "$1" 2>/dev/null
}

tehas_refresh_parked() { # <issue> <base>
  tehas_cycle_history "$1" | jq -se --argjson base "$2" \
    'any(.[]; .how == "refresh" and .parked == true and ((.started // 0) | todate) >= $base.at)' >/dev/null 2>&1
}

tehas_refresh_asked=""
tehas_refresh_answer=1
tehas_refresh_continues() {
  local caps
  if [ -z "$tehas_refresh_asked" ]; then
    tehas_refresh_asked=1
    caps=$(timeout -k 10 "${TEHAS_CAPS_WAIT:-120}" docker run --rm --init "${PI_ENSEMBLE_IMAGE:-}" \
      /opt/pi-rukas/bin/pi-rukas-headless --capabilities 2>/dev/null) || caps=""
    [ "$(tehas_headless_mode "$caps" continuePr)" != native ] || tehas_refresh_answer=0
  fi
  return "$tehas_refresh_answer"
}
