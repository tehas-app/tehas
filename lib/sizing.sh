# shellcheck shell=bash
# The measure of one issue body: four counts, read off its own text. Sourced,
# not run: the measure is one jq program over the body and asks nothing outside
# it. Nothing consumes it yet; the dispatcher's hold and the analytics read it
# later.
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
# given: a body of nothing is a body of four zeros.
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

tehas_issue_size() {
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
