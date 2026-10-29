# shellcheck shell=bash
# The pause flag, which every command that reads it reads through:
#
#   tehas_pause_read <file>       the flag tehas pause writes, {by, at, reason}
#   tehas_pause_oneline           the jq that puts a reason on one line
#
# The lines and the object of `tehas status` and the frame of `tehas top` are
# the console's: console/src/status.rs and console/src/frame.rs.

# A pause reason is printed as one line of tehas status and written to files a
# page may read, so it holds no line of its own: every control character (tab,
# carriage return, escape, the separators U+2028 and U+2029) becomes a space,
# and the ends are trimmed. The reader applies it and the line writer applies
# it again, so a reason that reached a file some other way forges no line.
tehas_pause_oneline='gsub("[\u0001-\u001f\u007f-\u009f\u2028\u2029]+"; " ") | gsub("^ *| *$"; "")'

# tehas_pause_read <file>: the pause flag, as tehas pause writes it.
#   0  <file> holds {by, at, reason}: the object, compact, on standard output
#   1  no such file: nothing on standard output
#   2  something is at <file> and is not a pause flag: nothing on standard output
# A flag that cannot be understood is never read as no flag: every caller counts
# 2 as paused, so a damaged flag stops the factory rather than starting it.
# `at` and `reason` must be strings; `by` may be anything and reads as nobody.
# Nothing that reaches a page publishes `by`: it names an account on this host.
# The reason comes back on one line, kept to the 200 characters tehas pause
# writes, whatever the file holds.
tehas_pause_read() { # <file>
  local json
  [ -e "$1" ] || return 1
  json=$(jq -ce '
    select(type == "object")
    | select((.at | type) == "string")
    | select((.reason | type) == "string")
    | {by: (if (.by | type) == "string" then .by else "" end), at,
       reason: (.reason | '"$tehas_pause_oneline"' | .[:200])}' "$1" 2>/dev/null) || return 2
  printf '%s\n' "$json"
}
