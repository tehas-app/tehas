# The five words of the park the state store lists last for one issue, joined
# with | as tehas_park_marker hashes them: step, cap, park reason, failure kind
# and failure cause. Nothing when that last end is no park of the step asked
# of ($step), or is the park of a provider outage, which has its own rule of
# retries. Equal hashes, equal parks, and no free text of a failure outside jq.
#
# The rows of history-show for one issue, of every kind, as the store answers
# them. lib/millwright.sh reads the file with jq -f; it holds no bash.
[ (.rows | map(select(.kind == "cycle")) | last) ] | .[0] as $p
| if $p == null or $p.parked != true or $p.provider == true then ""
  elif (($p.step // "" | gsub("[^a-z0-9-]"; "") | .[0:40]) != $step) then ""
  else [$p.step, $p.cap, $p.park_reason, $p.failure_kind, $p.failure_cause]
    | map(. // "") | join("|") end
