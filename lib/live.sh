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
# Called by tehas work each time it writes its own file, and by tehasbot
# health. Two callers at once can leave the view one tick old; the next tick
# corrects it. Needs TEHAS_HOME.

tehas_live_assemble() {
  local live=$TEHAS_HOME/live f files=() now marker tmp issues i
  now=$(date +%s)
  for f in "$live"/cycles/*.json; do
    [ -e "$f" ] || continue
    # A cycle writes every ten seconds. Two minutes of silence is a cycle
    # that died without saying so; it must not stay on the page as running.
    # It ended when it was found, so it counts from now, not from its last
    # heartbeat. The temporary name is not the one tehas work uses.
    if jq -e --argjson now "$now" '.running == true and ((.updatedAt // 0) < $now - 120)' "$f" >/dev/null 2>&1; then
      if jq -c --argjson now "$now" '.running = false | .status = "lost" | .updatedAt = $now' "$f" >"$f.lost.$$" 2>/dev/null; then
        mv "$f.lost.$$" "$f"
      else
        rm -f "$f.lost.$$"
      fi
    fi
    files+=("$f")
  done
  # Nothing written in this shape yet: leave what an earlier version wrote.
  [ "${#files[@]}" -gt 0 ] || return 0
  # A file newer than the marker was written after the files were read, so
  # the view says nothing about it and it is not pruned.
  marker=$live/running.json.mark.$$
  tmp=$live/running.json.$$
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
  if ! jq -c .view "$tmp" >"$tmp.view" 2>/dev/null || ! mv "$tmp.view" "$live/running.json"; then
    rm -f "$tmp" "$tmp.view" "$marker"
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
