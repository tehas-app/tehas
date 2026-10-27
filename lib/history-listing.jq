# The listing `tehas history` prints: the validation of each line, the cursor
# of each, and the count of a run of parks on the park that is one.
#
# libexec/history reads the ends off the state store, puts each in the shape
# its line had before the store (lib/history-lines.jq), and hands the lines
# here as one stream. This file answers the one object the command prints: the
# lines oldest first, each with the cursor a later --after reads from, and the
# count of the parks in a row that stopped exactly as it did.
#
# Needs $after: the cursor read so far, JSON null when the listing starts at
# the beginning, or the shape of one this command printed.
#
# The count lives in lib/history-repeats.jq, read here.
#
# The file is read with jq -f by libexec/history; it holds no bash.
include "history-repeats";
# jq turns a whole number over 2^53 into a decimal, where |floor is an error
# and comparison is unreliable. `whole` is the test that holds for every number
# this command writes and reads back, and the number is then taken as the text
# it was read as, which compares as a number does.
def whole($v): (($v | type) == "number") and ((("\($v)") | test("^[0-9]+$")) == true);
def iso_of($e): (if whole($e) then ($e | tostring | tonumber | todate) else null end);
def word($v): (if ($v | type) == "string" and $v != "" then $v else null end);
def bool($v): (if ($v | type) == "boolean" then $v else null end);
def num($v): (if whole($v) then ($v | tostring | tonumber) else null end);
def is_line: (type) == "object" and ((.kind | type) == "string")
  and ((.kind | test("^(work|plan)$")) == true)
  and whole(.issue) and ((.issue | tostring | tonumber) >= 1)
  and whole(.seq) and ((.seq | tostring | tonumber) >= 1)
  and ((.at | type) == "string")
  and ((.at | test("^[0-9]{4}-[0-9]{2}-[0-9]{2}T[0-9]{2}:[0-9]{2}:[0-9]{2}Z$")) == true)
  and ((.rc | type) == "number" or (.rc | type) == "string")
  and ((.status | type) == "string");
# A line is listed when it was written after the line of the cursor. An old
# cursor has no place, and is compared as before on the pair (the seconds of
# the end, the issue).
def after_cursor($c; $s): ($c | split("-")) as $p
  | if ($p | length) == 3 then (.seq | tostring | tonumber) > ($p[2] | tonumber)
    else ($s > ($p[0] | tonumber)) or
      (($s == ($p[0] | tonumber)) and ((.issue | tostring | tonumber) > ($p[1] | tonumber)))
    end;
# What a caller reads. A release that adds a field to a line adds it here too;
# nothing here renames or removes one. A line this release cannot read is left
# out, so an older or a newer file breaks no listing.
def publish($s): {cursor: "\($s)-\(.issue)-\(.seq)", kind, issue, ended: .at,
   started: iso_of(.started), status: num(.rc), outcome: .status, turn: num(.turn),
   step: word(.step), cap: word(.cap), parkReason: word(.park_reason),
   failureKind: word(.failure_kind), failureCause: word(.failure_cause),
   provider: bool(.provider), parked: bool(.parked), pr: word(.pr),
   by: word(.by), how: word(.how), label: word(.label),
   auto: (if .auto == true then true else null end),
   filed: (if (.filed | type) == "array" then
       [.filed[] | strings | select(test("^https://github\\.com/[^/]+/[^/]+/issues/[0-9]+$"))]
       | if length == 0 then null else . end
     else null end)}
  | with_entries(select(.value != null));
[ .[] | select(is_line) | (.at | try fromdateiso8601 catch null) as $s
  | select($s != null)
  | select(if $after == null then true else after_cursor($after; $s) end)
  | {seq: (.seq | tostring | tonumber), line: publish($s)} ]
# Oldest first, in the order the store wrote the ends: the order --after reads,
# so a program that keeps "next" and asks again is given each line once. The
# count of a run of parks is worked out on the listing, oldest first, and only
# of the lines it holds.
| sort_by(.seq) | map(.line)
| history_repeats as $c
| {api: 1, cycles: $c, next: (if ($c | length) == 0 then $after else $c[-1].cursor end)}
