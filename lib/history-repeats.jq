# The count of the parks in a row that stopped exactly as this one did, which
# `tehas history` puts on the park of such a run.
#
# A run is the parks of one issue that stopped exactly as each other did: step,
# cap and the three words of the failure all equal, each written just after the
# other. The first of them counts 1, the one after it 2, and so on. The line
# before a park is the end of the same issue written just before it, with the
# parks of a provider outage skipped: an outage has its own rule of retries, so
# it is never the park that starts a run and never the one that breaks it. A
# merge, a handoff or any other end does break a run. The count is worked out on
# the lines the listing holds and none besides, so a listing that begins partway
# through a run gives its first line no count: it cannot know what came before.
#
# The input is the listing, oldest first, as lib/history-listing.jq has
# published it: one object per end, with kind, issue, parked, provider and the
# park's five words. The output is the same lines, each of a run with `repeat`
# added. The file is read with jq -f, or included by name; it holds no bash.
def park($x): $x != null and $x.parked == true and $x.provider != true;
def same($a; $b): $a.step == $b.step and $a.cap == $b.cap and $a.parkReason == $b.parkReason
  and $a.failureKind == $b.failureKind and $a.failureCause == $b.failureCause;
# The line of the end of the same issue written just before the one at $i, the
# parks of a provider outage skipped, as the end this park is asked of: null
# when the issue has no such end in the listing.
def prev_of($all; $i): [ $all | keys_unsorted[]
    | select((. | tonumber) < ($i | tonumber)
              and ($all[.] | .kind == "work" and .issue == $all[$i].issue
                        and .provider != true)) ] | last;
# One pass over the listing, oldest first, keeping for each issue the line of
# its park seen last: a park of the same five words is one past that park. The
# counts answer by line index.
def history_repeats:
  . as $all
  | reduce (keys_unsorted[]) as $i ({n: {}, last: {}};
      $all[$i] as $e | ($e.issue | tostring) as $k
      | (prev_of($all; $i)) as $p | (if $p == null then null else $all[$p] end) as $b
      | (if (.last[$k] | type) == "number" and (.n[.last[$k] | tostring] | type) == "number"
         then .n[.last[$k] | tostring] else null end) as $before
      | if park($e) and park($b) and same($e; $b)
        then .n[$i | tostring] = (if $before == null then 1 else $before + 1 end) | .last[$k] = $i
        elif park($e) then .last[$k] = $i
        elif ($e.kind == "work" and $e.parked != true) then .last[$k] = null
        else . end)
  | .n as $n
  | [ $all | keys_unsorted[] as $i | $all[$i] as $e
      | ($n[$i | tostring] // 0) as $r | if $r > 0 then $e + {repeat: $r} else $e end ];
