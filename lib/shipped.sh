# shellcheck shell=bash
# shellcheck disable=SC2034  # tehas_shipped_label is read by the callers
# What closed an issue: the one GraphQL read that answers "a merged pull
# request closed this", and the one test that accepts the answer. Two commands
# act on it - tehasbot status (libexec/status) lists the shipped issues and
# gives them the label, and tehas tidy (libexec/tidy) puts the label on in the
# edit that takes the state labels off - so both read the query and the rule
# printed here, as lib/refresh.sh does for the pull requests a refresh works
# on: the shape of the timeline and the acceptance rule have one statement, and
# a change to either reaches the two at once.
#
# The closing pull request is not a field of an issue: it is in the issue's
# closed event, so this is GraphQL and not `gh issue list`.
#
#   tehas_shipped_label           the label's name
#   tehas_shipped_query           print the GraphQL query: the last closed
#                                 event of each issue the search $q names, and
#                                 whether its closer was a merged pull request.
#                                 $q must name every issue asked for
#   tehas_shipped_rule            print the jq definition of `shipped`: of one
#                                 issue node, 0 when its last closed event is
#                                 a merged pull request. The answer is read
#                                 with tehas_shipped_answer
#   tehas_shipped_answer <issue> <json>
#                                 0 when a whole search answered and named the
#                                 issue <issue>; 1 when it did not; 2 when it
#                                 answered and named no merged closer, which is
#                                 a closed issue, not a read that failed. A
#                                 response that carries top-level errors is
#                                 never a whole answer, even when it holds data
#
# Sourcing this file does nothing: it prints nothing and calls nothing.

tehas_shipped_label=tehas:shipped

tehas_shipped_query() {
  # shellcheck disable=SC2016  # $q is a GraphQL variable
  printf '%s\n' 'query($q: String!) { search(type: ISSUE, query: $q, first: 100) { nodes { ... on Issue {
    number timelineItems(itemTypes: [CLOSED_EVENT], last: 1) { nodes { ... on ClosedEvent {
      closer { ... on PullRequest { merged } } } } } } } } }'
}

tehas_shipped_rule() {
  # A closer is a merge only when GitHub says `merged` is the boolean true: a
  # string, a number or an object is no answer and is never taken for one.
  # shellcheck disable=SC2016  # a jq program
  printf '%s\n' 'def shipped: (.timelineItems.nodes // [])[0].closer.merged | . == true;'
}

tehas_shipped_answer() { # <issue> <answer of the search>
  local nodes
  nodes=$(jq -ce --argjson n "$1" "$(tehas_shipped_rule)
    if (.errors // null) != null then error(\"declined\")
    elif (.data.search.nodes | type) != \"array\" then error(\"no list\")
    else . end
    | [(.data.search.nodes)[] | select(.number == \$n) | select(shipped)]
    | length" <<<"${2:-}" 2>/dev/null) || return 1
  [ "${nodes:-0}" != 0 ] && return 0
  return 2
}
