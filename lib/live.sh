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
  local live=$TEHAS_HOME/live f files=() now keep tmp
  now=$(date +%s)
  for f in "$live"/cycles/*.json; do
    [ -e "$f" ] || continue
    # A cycle writes every ten seconds. Two minutes of silence is a cycle
    # that died without saying so; it must not stay on the page as running.
    if jq -e --argjson now "$now" '.running == true and ((.updatedAt // 0) < $now - 120)' "$f" >/dev/null 2>&1; then
      if jq -c '.running = false | .status = "lost"' "$f" >"$f.new" 2>/dev/null; then mv "$f.new" "$f"; fi
    fi
    files+=("$f")
  done
  # Nothing written in this shape yet: leave what an earlier version wrote.
  [ "${#files[@]}" -gt 0 ] || return 0
  tmp=$live/running.json.$$
  if ! jq -c -s '{cycles: (map(select(.running == true)) | sort_by(.startedAt, .issue)),
                   last: (map(select(.running != true)) | sort_by(.updatedAt) | last)}' "${files[@]}" >"$tmp" 2>/dev/null; then
    rm -f "$tmp"
    return 1
  fi
  mv "$tmp" "$live/running.json" || return 1
  # Only the last finished cycle is shown, so only its file is kept.
  keep=$(jq -r '.last.issue // empty' "$live/running.json" 2>/dev/null)
  for f in "${files[@]}"; do
    [ "${f##*/}" = "$keep.json" ] && continue
    if jq -e '.running != true' "$f" >/dev/null 2>&1; then rm -f "$f"; fi
  done
  return 0
}
