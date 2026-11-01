# shellcheck shell=bash
# Refreshes: the factory's own pull requests that conflict with the branch.
# Sourced by tehasbot refresh (libexec/refresh), which finds them. The
# dispatcher checks each again before it starts one, by the same rule in Rust
# (console/dispatch/src/decide/retry.rs).
#
#   tehas_refresh_dir             state/refresh, where the requests are
#   tehas_refresh_query           print the GraphQL query of the open pull
#                                 requests of the repository, with what the
#                                 rule reads of each and of the issue it closes
#   tehas_refresh_rule            print a jq definition, `refresh_why`: of one
#                                 pull request node, why it is not one the
#                                 factory refreshes, "" when it is. Needs $bot
#                                 (an app's [bot] suffix is dropped: GraphQL
#                                 names an app without it),
#                                 $repo and $control as jq arguments. The
#                                 mergeable state is the caller's to read
#
# Needs TEHAS_HOME. Sourcing it does nothing.

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

# Only the bot's own pull request of one open issue that waits for a merge or
# was handed off (needs-human-attention) and has no cycle running, from a
# branch of the repository, and the only open one that closes that issue. A
# draft is refreshed like any other pull request, and its draft state is left
# as it is; a draft state the answer does not name is a shape the rule does not
# know, and a shape it does not know is a reason too.
tehas_refresh_rule() {
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def refresh_why:
    def names: [(.labels.nodes // [])[]? | .name? | strings];
    if type != "object" or (.number | type) != "number" then "it cannot be read"
    elif (.author.login // "") != ($bot | sub("\\[bot\\]$"; "")) then "it is not the bot'"'"'s"
    elif .isCrossRepository != false or ((.headRepository.nameWithOwner // "") | ascii_downcase) != ($repo | ascii_downcase) then "its branch is not in \($repo)"
    elif .isDraft == null then "its draft state cannot be read"
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
