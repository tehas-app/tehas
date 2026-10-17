#!/usr/bin/env bash
# Tests for libexec/preview: the notice at the top of a pull request's
# description, and the text around it that is left alone.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh"

t_case "a serving preview is said once, at the top of the description"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "api -X PATCH repos/acme/widget/pulls/12 -f body=<!-- tehas-preview -->"
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

What the change does."
t_run "$preview"
t_eq "$(writes | wc -l)" 1

t_case "a pull request with no description gets the notice and nothing else"
reset
desc=""
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")"

t_case "the rest of the description is kept exactly as GitHub gave it"
reset
desc='Closes #7.

```sh
echo "hi"
```'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

$desc"

t_case "a description that ends in a blank line keeps it, and so does its copy"
reset
desc='Fixes #7.

'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
# The last two bytes of what GitHub holds and of the copy kept on the host.
# A command substitution would have taken them off.
t_eq "$(bytes "$T/fx/desc-12" 2)" "0a0a"
t_eq "$(bytes "$root/12/.description" 2)" "0a0a"
# The copy is of a description that may hold anything its author wrote, so it is
# readable by the tehas user only.
t_eq "$(stat -c '%a' "$root/12/.description")" "600"

t_case "a new commit edits the notice in place and asks GitHub for the description it has"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
a2=$(commit feature)
desc="$(told 12)"
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(writes | wc -l)" 2
t_contains "$(writes | tail -n 1)" "api -X PATCH repos/acme/widget/pulls/12"
t_lacks "$(told 12)" "${a:0:7}"
t_eq "$(told 12)" "$(block "Preview of \`${a2:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

What the change does."
# The write is said, and what GitHub had before it is kept on the host.
t_contains "$t_out" "#12: the notice was written"
t_contains "$(cat "$root/12/.description")" "${a:0:7}"
t_lacks "$(cat "$root/12/.description")" "${a2:0:7}"

t_case "a description that already says this is not written again"
reset
a=$(commit feature)
desc="$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")
What the change does."
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_eq "$(cat "$root/12/.notice")" "$a serving none 0 $when"

# A marker line of someone's own, anywhere but as the notice at the very top,
# is text this command does not touch.
for odd in $'<!-- tehas-preview -->\nopen with no close' \
           $'<!-- /tehas-preview -->\nclose with no open' \
           $'Their notice\n<!-- tehas-preview -->\na\n<!-- /tehas-preview -->' \
           $'<!-- tehas-preview -->\na\n<!-- /tehas-preview -->\n<!-- tehas-preview -->\nb\n<!-- /tehas-preview -->'; do
  t_case "a description with a marker line of its own ('${odd:0:26}') is left alone"
  reset
  desc=$odd
  a=$(commit feature)
  pulls "$(pull 12 "$a")"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(state_of 12)" "serving"
  t_eq "$(writes)" ""
  t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"
  t_eq "$(printf '%s\n' "$t_out" | grep -c 'left alone')" 1
  t_eq "$(told 12)" ""
  t_contains "$(cat "$root/12/.notice")" "$a serving refused"
  # Said once for each commit and state: the description is not asked about
  # again, and nothing is written, until one of them changes.
  rm -f "$T"/calls/gh
  t_run "$preview"
  t_lacks "$t_out" "left alone"
  t_eq "$(writes)" ""
  t_eq "$(t_calls gh | grep -c 'pulls/12')" 0
done

# A block of the marker lines at the very top is not this command's unless
# everything between them is one of the notices it writes.
t_case "a block at the very top that this command did not write is left alone"
reset
desc='<!-- tehas-preview -->
Security review pending, do not merge.
<!-- /tehas-preview -->

Real description here.'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"
t_run "$preview"
t_eq "$(writes)" ""
# What was said about that commit and state stands while GitHub gives the same
# time for the pull request as it did at the last look: no read, no write, no
# line.
desc='Real description here.'
pulls "$(pull 12 "$a")"
rm -f "$T"/calls/gh
t_run "$preview"
t_eq "$(writes)" ""
t_eq "$(t_calls gh | grep -c 'pulls/12')" 0
# With the marker lines gone and a later time from GitHub, the same commit and
# state do not wait for a new commit: the notice comes at the run that sees the
# description change.
desc='Real description here.'
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

Real description here."
t_eq "$(cat "$root/12/.notice")" "$a serving said 0 $when"

# Text between the marker lines that this command did not write is not written
# over, and the block is not taken as its own: all of it or none of it.
t_case "text a person put inside the marker lines of a notice is left alone"
reset
desc='<!-- tehas-preview -->
Preview of `deadbeef`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.
Do not merge before Friday.
<!-- /tehas-preview -->

Real description here.'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"
t_eq "$(told 12)" ""

# A closing marker of its own below a notice of ours does not stop the notice:
# the block at the top is replaced and the rest stays as GitHub gave it.
t_case "a marker line below a notice of ours keeps its place in the description"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
desc="$(told 12)

Someone quoted the closing marker: <!-- /tehas-preview -->"
a2=$(commit feature)
pulls "$(pull 12 "$a2")"
t_run "$preview"
t_eq "$(writes | wc -l)" 2
t_eq "$(told 12)" "$(block "Preview of \`${a2:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

What the change does.

Someone quoted the closing marker: <!-- /tehas-preview -->"

# A notice at the top that mimics this command's is corrected, not kept.
t_case "a block at the top that gives another address is written over"
reset
desc='<!-- tehas-preview -->
Preview of `deadbeef`: https://not-the-preview.example/

It is rebuilt when the branch changes and removed when this pull request is closed.
<!-- /tehas-preview -->
<!-- /tehas-preview -->
Real description here.'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(told 12)" "https://uat-12.example.org/"
t_lacks "$(told 12)" "not-the-preview"

# Ownership of a description is only its text, so it is looked at again whenever
# GitHub says the pull request moved. A block written by someone else, in the
# shape of the bot's, after the bot had spoken is found at the next run: it does
# not stand until someone pushes.
t_case "a block someone put at the top after this command spoke is written over at the next run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_contains "$t_out" "#12: the notice was written"
desc='<!-- tehas-preview -->

Preview of `deadbeef`: https://not-the-preview.example/

It is rebuilt when the branch changes and removed when this pull request is closed.
<!-- /tehas-preview -->
What the change does.'
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
rm -f "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(writes)" "api -X PATCH repos/acme/widget/pulls/12"
t_contains "$(told 12)" "https://uat-12.example.org/"
t_lacks "$(told 12)" "not-the-preview"
t_lacks "$(told 12)" "deadbeef"
t_eq "$(told 12 | tail -n 1)" "What the change does."
t_eq "$(cat "$root/12/.notice")" "$a serving said 0 $when"
# The record carries the time of that look, so the run after it looks no more.
rm -f "$T"/calls/gh "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 0
t_eq "$(writes)" ""

t_case "a notice a person removed comes back at the next run"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(count "$root/12" .notice)" 1
desc='What the change does. And a note added later.'
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
rm -f "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_eq "$(told 12)" "$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")

What the change does. And a note added later."

# A pull request moves for a comment or a label as much as for a description.
# The look that follows costs one read and nothing else: no write, and no line
# in the log.
t_case "a pull request that moved with its description unchanged is read once and written not at all"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
# The description GitHub holds is the one the command wrote: only the pull
# request moved, for a comment or a label.
desc="$(told 12)"
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
rm -f "$T"/calls/gh "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 1
t_eq "$(writes)" ""
t_lacks "$t_out" "notice"
t_eq "$(cat "$root/12/.notice")" "$a serving none 0 $when"
rm -f "$T"/calls/gh
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 0

# A description this command cannot own is looked at again when GitHub says it
# moved, and still left alone. The line about it is said once for that commit
# and state, not once for every look.
t_case "a description still not this command's is looked at again and said about once"
reset
desc='<!-- tehas-preview -->
Security review pending, do not merge.
<!-- /tehas-preview -->'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(printf '%s\n' "$t_out" | grep -c 'left alone')" 1
desc='<!-- tehas-preview -->
Security review pending, do not merge.
And a line added later.
<!-- /tehas-preview -->'
when="2026-10-11T09:20:00Z"
pulls "$(pull 12 "$a")"
rm -f "$T"/calls/gh "$T/calls/gh-writes"
t_run "$preview"
t_eq "$(t_calls gh | grep -c 'pulls/12')" 1
t_eq "$(writes)" ""
t_eq "$(printf '%s\n' "$t_out" | grep -c 'left alone')" 0
t_contains "$(cat "$root/12/.notice")" "$a serving refused"

# A description that is only the notice, with the newline its closing marker
# line ends with, already says what it should say: it is not written again, and
# its last line is not eaten.
t_case "a description that is only the notice is not written again"
reset
a=$(commit feature)
desc="$(block "Preview of \`${a:0:7}\`: https://uat-12.example.org/

It is rebuilt when the branch changes and removed when this pull request is closed.")
"
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(writes)" ""
t_eq "$(cat "$root/12/.notice")" "$a serving none 0 $when"

# A carriage return at the end of a line does not hide a marker line, and does
# not make a second notice.
t_case "a description whose lines end with a carriage return is read as one description"
reset
desc=$'<!-- tehas-preview -->\r\nPreview of `deadbeef`: https://uat-12.example.org/\r\n\r\nIt is rebuilt when the branch changes and removed when this pull request is closed.\r\n<!-- /tehas-preview -->\r\nWhat the change does.\r\n'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_eq "$(told 12 | grep -c -x -- '<!-- tehas-preview -->')" 1
t_eq "$(told 12 | grep -c -x -- '<!-- /tehas-preview -->')" 1
t_contains "$(told 12)" "Preview of \`${a:0:7}\`: https://uat-12.example.org/"
t_lacks "$(told 12)" "deadbeef"
t_eq "$(told 12 | tail -n 1)" "What the change does."$'\r'
t_run "$preview"
t_eq "$(writes | wc -l)" 1

t_case "a block of carriage returns that is not this command's is left alone"
reset
desc=$'<!-- tehas-preview -->\r\nSecurity review pending, do not merge.\r\n<!-- /tehas-preview -->\r\nReal description here.\r\n'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes)" ""
t_contains "$t_out" "#12: the description has a preview notice that is not this one, left alone"

t_case "a marker line inside someone's sentence does not stop the notice"
reset
desc='A preview lives at <!-- tehas-preview --> somewhere in here.'
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(writes | wc -l)" 1
t_contains "$(told 12)" "Preview of \`${a:0:7}\`: https://uat-12.example.org/"
t_eq "$(told 12 | tail -n 1)" "A preview lives at <!-- tehas-preview --> somewhere in here."

t_done
