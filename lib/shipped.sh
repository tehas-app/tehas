# shellcheck shell=bash
# shellcheck disable=SC2034  # the names here are read by the callers
# What closed an issue: the one GraphQL read that answers "a merged pull
# request closed this", and the names its callers read it by. Two commands act
# on it - tehasbot status (libexec/status) lists the shipped issues and gives
# them the label, and tehas tidy (libexec/tidy) puts the label on in the edit
# that takes the state labels off - so both take the query and the ceiling of a
# page from here, as lib/refresh.sh does for the pull requests a refresh works
# on: the shape of the timeline and the ceiling have one statement, and a change
# to either reaches the two at once. What an answer means is the console's, in
# console/src/shipped.rs, for the same reason: one rule decides both commands.
#
# The closing pull request is not a field of an issue: it is in the issue's
# closed event, so this is GraphQL and not `gh issue list`. The call stays bash
# because a caller makes it under its own time limit and once for a whole poll,
# and the answer goes to `console shipped`, which decides it.
#
#   tehas_shipped_label             the label's name
#   tehas_shipped_page <console>    how many issues one read of the search may
#                                   name: the page of the query and the ceiling
#                                   of the list a caller builds. The two are one
#                                   number so they cannot drift apart. GitHub
#                                   puts a ceiling on the terms of a search and
#                                   the read carries one per issue, so a longer
#                                   list is not truncated: it is no answer at all
#   tehas_shipped_query <console>   print the GraphQL query: the last closed
#                                   event of each issue the search $q names,
#                                   whether its closer was a merged pull
#                                   request, and the names of the labels the
#                                   issue holds. $q must name every issue asked
#                                   for, and no more than tehas_shipped_page of
#                                   them
#   tehas_shipped_list <console> [--control <number>]... <answer on standard input
#                                   the nodes of the answer whose closer was a
#                                   merged pull request, one JSON object on a
#                                   line, most recently closed first - the order
#                                   the dashboard lists them in, and what
#                                   tehasbot status shows. Each <control> is not
#                                   work and is left out, as both commands leave
#                                   the control issue out of every list and count.
#                                   The status is the console's: 0 when the answer
#                                   named at least one, and 1 when it named none,
#                                   carried errors or was no JSON at all - no
#                                   label is earned anywhere in a poll whose read
#                                   answered with 1, and the log says the read
#                                   failed rather than that no closer was merged
#   tehas_shipped_numbers <console> [--control <number>]... <answer on standard
#                                   input
#                                   the same list with the number of each node on
#                                   a line and nothing else, which is all tehas
#                                   tidy asks and the cheaper line to read
#   tehas_shipped_whole <console> <answer>
#                                   whether <answer> is a reply of the read at
#                                   all: the console's test of the answer, the one
#                                   `list` makes before it prints anything. 0 when
#                                   it names a list of nodes, and 1 when it carries
#                                   errors, names no list or is no JSON. A caller
#                                   that took the list above tells a page of nothing
#                                   (an answer) from a failed read (not one)
#   tehas_shipped_has <console> <node> <label>
#                                   of one node of that query, as the caller's
#                                   status: 0 when <label> is one of the names the
#                                   node holds, 1 when the read named its labels
#                                   and the label is none, and 2 when it named no
#                                   list of them. The word the console prints with
#                                   them is the console's own and no caller reads
#                                   it, so this line sends it away: what a poll
#                                   writes to standard error is what its log
#                                   holds. A caller edits on the 2 as on the 1, and
#                                   never guesses that an issue it cannot read
#                                   lacks the label
#
# A closer is a merge only when GitHub says `merged` is the boolean true, a label
# is one name compared as text, and a whole answer one that carries no errors and
# a list of nodes: all of it is said in console/src/shipped.rs and nowhere here.
# Sourcing this file does nothing. A command that reads an answer checks its own
# console is executable before its first call, as tehas clean checks its own,
# because a poll that cannot ask is a poll that labels nothing.

tehas_shipped_label=tehas:shipped

tehas_shipped_page() { # <console>
  "$1" shipped page
}

tehas_shipped_query() { # <console>
  "$1" shipped query
}

# The two list verbs are the console's `list`, with its own flags passed through
# as it writes them: `--control 1` is one number and not a list of them. The
# answer is a pipe, so the one read of a poll is asked once and never reaches a
# command line.
tehas_shipped_list() { # <console> [--control <number>]... <answer
  local console=$1
  shift
  "$console" shipped list "$@"
}

tehas_shipped_numbers() { # <console> [--control <number>]... <answer
  local console=$1
  shift
  "$console" shipped list --numbers "$@"
}

# Whether the reply names a list of nodes, and nothing else: the console's own
# test of the answer, the one `list` makes before it prints anything.
tehas_shipped_whole() { # <console> <answer>
  printf '%s' "${2:-}" | "${1:-}" shipped whole >/dev/null 2>&1
}

tehas_shipped_has() { # <console> <node> <label>
  "${1:-}" shipped has "${3:-}" "${2:-}" 2>/dev/null >/dev/null
}
