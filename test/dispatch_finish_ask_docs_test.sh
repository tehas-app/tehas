#!/usr/bin/env bash
# Tests for host/docs/troubleshooting.md: the section "Nothing happens after I
# label an issue" names the four lines the poll's finish step logs, in the words
# console/dispatch/src/finish.rs logs them, and says when the step runs at all.
# The behaviour itself is the Rust tests of that file and
# test/dispatch_finish_ask_test.sh.
# shellcheck disable=SC2016  # the lines below are literal text of the page
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

page=$T_ROOT/host/docs/troubleshooting.md

# The section's own text: from its heading to the next heading of any level, so
# a case that drops a line from this section fails and one from below does not.
section=$(awk '/^## Nothing happens after I label an issue$/ { on = 1; next }
              on && /^#/ { exit }
              on { print }' "$page")

# One line of the page with its soft wraps joined, so a line broken across two
# source lines still reads as the log line it quotes.
unfold() { tr '\n' ' ' <<<"$section" | sed -e 's/  */ /g'; }
flat=$(unfold)

# The four lines finish.rs logs for one ask. A bullet names each of them in the
# words of the source, with N for the issue number the section uses.
for line in \
  "ignore #N: tehas:finish was applied by '<who>', not an allowed account" \
  "wait #N: tehas:finish waits: <why>" \
  "finish #N: refused, asked by <who>: <why>" \
  "finish #N: refused, asked by <who> (<label|millwright>): <why>" \
  "finish #N: opened <url>, asked by <who> (<label|millwright>): <reason>"; do
  t_case "the section names the finish line: $line"
  t_contains "$flat" "$line"
done

t_case "each of the four bullets links the operating page"
t_eq "$(grep -c '(/operating#finishing-a-stopped-cycle)' <<<"$section")" 4

t_case "the section says the step runs after the poll's decisions, takes three asks at most and no cycle"
t_contains "$flat" "The finish step runs after the poll's own decisions"
t_contains "$flat" "takes at most three asks a poll"
t_contains "$flat" "starts no cycle"

t_case "the section says a paused, dry or failed poll runs no finish step"
t_contains "$flat" "does not run on a poll that is paused, dry or that failed"

t_case "the page holds no address of a running host"
t_lacks "$(cat "$page")" "https://tehas.app"

t_done
