# shellcheck shell=bash
# The live view of the cycles: $TEHAS_HOME/live/running.json, read by the
# factory floor, the plain dashboard, tehasbot health and tehas dev.
#
# Each cycle writes a file of its own, live/cycles/<issue>.json, every ten
# seconds: one file cannot have several writers. The view is put together
# from those files:
#
#   {"cycles": [<each running cycle, oldest first>],
#    "last": <the cycle that finished most recently, or null>}
#
#   tehas_live_assemble   write live/running.json; returns 1 if it could not
#
# A file that went quiet is lost only when its issue's lock
# (state/cycles/<issue>.lock, held by tehasbot cycle) is free: a host that
# stalled for two minutes does not make a running cycle lost.
#
# Called by tehas work each time it writes its own file, and by tehasbot
# health. Two callers at once can leave the view one tick old; the next tick
# corrects it. Needs TEHAS_HOME.

tehas_live_assemble() {
  local live=$TEHAS_HOME/live f files=() now marker tmp view issues i lost lock
  now=$(date +%s)
  for f in "$live"/cycles/*.json; do
    [ -e "$f" ] || continue
    # A cycle writes every ten seconds. Two minutes of silence is a cycle
    # that died without saying so; it must not stay on the page as running.
    # It ended when it was found, so it counts from now, not from its last
    # heartbeat. The temporary names start with a dot and end in .new: the
    # docs server hides those, and the glob above does not match them.
    lock=${f##*/}
    lock=$TEHAS_HOME/state/cycles/${lock%.json}.lock
    if jq -e --argjson now "$now" '.running == true and ((.updatedAt // 0) < $now - 120)' "$f" >/dev/null 2>&1 &&
      ! { [ -e "$lock" ] && ! flock -n "$lock" true; }; then
      lost=${f%/*}/.${f##*/}.lost.$$.new
      if jq -c --argjson now "$now" '.running = false | .status = "lost" | .updatedAt = $now' "$f" >"$lost" 2>/dev/null; then
        mv "$lost" "$f"
      else
        rm -f "$lost"
      fi
    fi
    files+=("$f")
  done
  # Nothing written in this shape yet: leave what an earlier version wrote.
  [ "${#files[@]}" -gt 0 ] || return 0
  # A file newer than the marker was written after the files were read, so
  # the view says nothing about it and it is not pruned.
  marker=$live/.running.$$.mark.new
  tmp=$live/.running.$$.new
  view=$live/.running.$$.view.new
  : >"$marker"
  if ! jq -c -s '
      (map(select(.running == true)) | sort_by(.startedAt, .issue)) as $cycles
      | (map(select(.running != true)) | sort_by(.updatedAt) | last) as $last
      | {view: {cycles: $cycles, last: $last},
         prune: (map(select(.running != true and .issue != $last.issue) | .issue))}' \
      "${files[@]}" >"$tmp" 2>/dev/null; then
    rm -f "$tmp" "$marker"
    return 1
  fi
  issues=$(jq -r '.prune[]' "$tmp" 2>/dev/null)
  if ! jq -c .view "$tmp" >"$view" 2>/dev/null || ! mv "$view" "$live/running.json"; then
    rm -f "$tmp" "$view" "$marker"
    return 1
  fi
  # Only the last finished cycle is shown, so only its file is kept.
  for i in $issues; do
    f=$live/cycles/$i.json
    [ -e "$f" ] || continue
    [ "$f" -nt "$marker" ] && continue
    rm -f "$f"
  done
  rm -f "$tmp" "$marker"
  return 0
}
