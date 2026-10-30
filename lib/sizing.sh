# shellcheck shell=bash
# The measure of one issue body: four counts, read off its own text. Sourced,
# not run: the measure is one jq program over the body and asks nothing outside
# it. tehas work measures the body of an approved start and keeps the answer in
# state/sizes.jsonl; console analytics report reads that ledger.
#
#   tehas_issue_size    stdin: one issue body as text. stdout: one compact
#                       JSON object, one line, keys always in this order:
#                         {"files": n, "tests": n, "deliverables": n,
#                          "done_when": n}
#
# The four counts, and what each of them is:
#
#   files         distinct repository paths written between two tick characters
#                 (the character that quotes a path in markdown) that hold a
#                 "/" or end in .sh, .rs, .md, .json, .toml or .yml. A bare
#                 tehas work and notes.txt are not paths. A path is a path
#                 wherever it is written, inside a fenced block or not. A path
#                 that appears only on a line starting with "Not ", "Do not "
#                 or "Leave " is not counted; the same path elsewhere still is.
#                 A pair of ticks that straddles a line break counts for
#                 nothing.
#   tests         the paths of files that start with test/
#   deliverables  the lines of an ordered list, "1. ", "2. " and so on, under a
#                 heading whose text is "The change", "Deliverable(s)" or "What
#                 to change", and up to the next heading of any level
#   done_when     the items of an unordered list, "- " or "* ", under a heading
#                 whose text is "Done when", and up to the next heading of any
#                 level
#
# A body is markdown. A heading is a line that begins with 1 to 6 # and is read
# as one: not a line inside a fenced block, and not one indented four spaces or
# more, which markdown reads as code. Inside a fenced block a path is still a
# path and a numbered line is prose.
#
# The headings are matched whole, so "Changes to make" and "Files" are neither.
# A line sits under the last heading at or before it, so a heading governs its
# own line and every line after it, up to the next heading of any level and the
# end of the body. What stands before the first heading belongs to no section.
#
# Pure: no network, no git, no file opened but standard input, nothing written,
# no setting read. It takes no argument and prints one line whatever it is
# given: a body of nothing is a body of four zeros. It is bounded by the two
# settings above, because the pass it makes is not free.
#
# Three cautions on the program below, all bash rather than jq. It is one
# single-quoted argument, so no backtick may appear in it: the tick and the
# fence come in as arguments. No line may put a "]" before a ")", which
# together read as the start of a command substitution. And inside a reduce the
# state is the result, not the input, so a line cannot be indexed there by its
# own position: these passes carry what they need in the value they build.

# The tick and the fence of markdown, written by their octal codes.
tehas_tick=$(printf "\140")
tehas_fence=$(printf "\140\140\140")

# The measure is bounded by two settings, an instance's to set, because the one
# pass it makes costs more the more lines it reads and, on a line of its own,
# the more bytes: TEHAS_SIZE_MAX_BYTES (131072) and TEHAS_SIZE_MAX_LINES
# (1000). Neither bound alone holds the other's worst case, a body of one huge
# line and a body of many short ones. The four counts are coarse, so a body
# past either is measured from its front, and a truncation is no reason to stop
# a start: the answer is still four numbers. Both bounds are set well below what
# an issue's text holds in practice.
#
# One body, cut to its front on both bounds before jq reads a line of it. The
# byte cut falls at a byte and so can split one character; jq replaces what it
# then cannot decode, and no path, list item or heading spans a multi-byte
# character.
tehas_issue_size() {
  head -c "${TEHAS_SIZE_MAX_BYTES:-131072}" | head -n "${TEHAS_SIZE_MAX_LINES:-1000}" | \
    jq -Rrs --arg tick "$tehas_tick" --arg fence "$tehas_fence" '
    def head: test("^#{1,6}[ \t]*[^ \t]");
    def under($re): test("^#{1,6}[ \t]*(?i)" + $re + "[ \t]*$");
    # What stands between two ticks on one line. Built from the argument: the
    # class excludes the tick itself, so it cannot be written in the program.
    def between: $tick + "([^" + $tick + "]+)" + $tick;
    # A line that opens or closes a fenced block: three of the fence character
    # or more, or ~~~, with up to three spaces in front.
    def fence: test("^[ \t]{0,3}(" + $fence + "|~~~)");
    # Whether the body runs at a line: not inside a fenced block, and not
    # indented four spaces or more. The fence lines divide the body, so a block
    # that holds a heading of its own neither moves a section nor ends one:
    # only a fence the body runs at opens or closes a block, and a line is read
    # when an even number of them stand before it.
    def runs: reduce .[] as $row ([];
        . as $seen
        | ($seen | length) as $n
        | ([ $seen[] | select(.fence and .open) ] | length % 2 == 0) as $out
        | . + [ $row
                + { fence: ($row.line | fence),
                    open: ($out and ($row.line | fence)),
                    run: ($out and ($row.line | fence | not)
                          and ($row.line | test("^    ") | not)) } ]);
    # The sections. A heading governs its own line and every line after it, up
    # to the next heading of any level and the end of the body. A function, not
    # an inline reduce: the state of a reduce is its result, so the flags of
    # the line before are read from the value being built, not indexed.
    def sections: reduce .[] as $row ([];
        (if $row.run and ($row.line | head) then
           { change: ($row.line | under("(the )?(change|deliverables?|what to change)")),
             done: ($row.line | under("done when")) }
         else
           { change: (.[-1].change // false), done: (.[-1].done // false) }
         end) as $here
        | . + [ $row + $here ]);
    # The body, one row per line. splits, not split: given the body as one
    # string, a fence and a path would both reach over a line break.
    [ splits("\n") | { line: . } ] | runs | sections as $s
    # A line whose first words say a path is not to be touched gives up every
    # path on it; one that says nothing of the kind gives up none.
    | ([ $s[].line | select(test("^(Not|Do not|Leave) ") | not) | scan(between) ]
        | flatten | map(select(test("/") or test("[.](sh|rs|md|json|toml|yml)$")))
        | unique) as $paths
    | { files: ($paths | length),
        tests: ([ $paths[] | select(test("^test/")) ] | length),
        deliverables: ([ $s[] | select(.change and .run) | .line | select(test("^[0-9]+[.] ")) ] | length),
        done_when: ([ $s[] | select(.done and .run) | .line | select(test("^[-*] ")) ] | length) }
    # tojson, so the answer is jq compact form: one line, keys in this
    # order, and none of the body text in it.
    | tojson
  '
}

# The ledger of what an approved start measured: state/sizes.jsonl, one line
# per start that had an approval to measure, {at, issue, files, tests,
# deliverables, done_when}. tehas work is the only writer; console analytics
# report reads it in its sizes section, so every line earns its keep.
#
# tehas_size_record <issue> <body> <file>: the measure of <body>, keyed with
# the time and the issue number, appended to <file>. The line is built in one
# jq pass over the measure, and kept only when all four counts are there and
# are numbers, so a measure that answered other than the four is no line rather
# than a line of two keys. <file> is opened owner-only, whatever the umask, and
# is written only when it is a plain file and no link there, the way
# tehas_record_is_file reads the approval record. The ledger keeps the last 500
# lines, as the cycle's history keeps 50: the file is one per instance, not one
# per issue.
#
# Needs tehas_record_is_file, which lib/attachments.sh gives.
#
# A line that cannot be written is one word on this command's error stream and
# 1: the measure is a record of the start, never a reason to stop one.
# $tehas_size_why then says which step gave up, in one word.
tehas_size_why=""
# shellcheck disable=SC2034  # the caller reads what this sets
tehas_size_record() { # <issue> <body> <ledger>
  local size line wrote=0 trimmed=0 before
  tehas_size_why=""
  # The measure, then the line it becomes: the four counts and the two keys
  # that say when and of which issue.
  size=$(tehas_issue_size <<<"${2:-}" 2>/dev/null) || size=""
  line=$(jq -cn --arg at "$(date -u +%FT%TZ)" --argjson issue "$1" --argjson s "$size" \
    '$s | {at: $at, issue: $issue} + {
      files: (.files | select(type == "number")),
      tests: (.tests | select(type == "number")),
      deliverables: (.deliverables | select(type == "number")),
      done_when: (.done_when | select(type == "number"))}' 2>/dev/null) || line=""
  if [ -z "$line" ]; then tehas_size_why=measure; return 1; fi
  # What may be written to: a plain file and no link, the rule
  # tehas_record_is_file gives for the approval record. A path that is nothing
  # yet is the ledger's first line, and is made here, owner-only.
  if [ -e "$3" ] || [ -L "$3" ]; then
    tehas_record_is_file "$3" || { tehas_size_why=ledger; return 1; }
  else
    wrote=1
    (umask 077; : >>"$3") 2>/dev/null || { tehas_size_why=ledger; return 1; }
  fi
  # The write is judged by what the file ends with and by how many lines it
  # holds, not by what the redirect reported: a shell swallows a redirect's own
  # error, and the line already there is one fewer than the file holds once
  # this one is on it.
  before=$(wc -l <"$3" 2>/dev/null)
  (umask 077; printf '%s\n' "$line" >>"$3") 2>/dev/null
  if [ "$(tail -n 1 "$3" 2>/dev/null)" != "$line" ] ||
    [ "$(wc -l <"$3" 2>/dev/null)" != "$(( ${before:-0} + 1 ))" ]; then
    # Only a file this call made is removed: a ledger it could not add to keeps
    # what it holds.
    if [ "$wrote" = 1 ]; then rm -f "$3" 2>/dev/null; fi
    tehas_size_why=ledger
    return 1
  fi
  # The ledger keeps the last 500 lines, as the cycle's history keeps 50: the
  # file is one per instance, not one per issue. A trim that cannot be made
  # leaves the ledger a line longer, which is no loss of a start.
  if [ "$(wc -l <"$3" 2>/dev/null)" -gt 500 ]; then
    tail -n 500 "$3" >"$3.new" 2>/dev/null && mv "$3.new" "$3" 2>/dev/null || trimmed=1
  fi
  if [ "$trimmed" = 1 ]; then
    tehas_size_why=ledger
    return 1
  fi
}
