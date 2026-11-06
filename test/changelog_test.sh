#!/usr/bin/env bash
# Tests for scripts/collect-changelog.sh (xtask collect-changelog) and the
# changelog fragments that scripts/release-notes.sh reads for the Unreleased
# section. console/xtask/src/changelog/tests.rs has the rules over text.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# The scripts exec the xtask of this checkout, built once and named in
# TEHAS_XTASK.
t_xtask || t_bad "no xtask: $t_xtask_why"

collect=$T_ROOT/scripts/collect-changelog.sh
notes=$T_ROOT/scripts/release-notes.sh
log=$T/log/CHANGELOG.md
frags=$T/log/changelog.d

# A changelog with an Unreleased section and one release, and no fragments.
fresh() {
  rm -rf "$T/log"
  mkdir -p "$frags"
  printf '%s\n' '# Changelog' '' '## Unreleased' '' '### Changes' '' '- in the file' '' \
    '### Adopter action' '' 'None.' '' '## v1.2.3 (2026-10-21)' '' '### Changes' '' '- old' '' \
    '### Adopter action' '' 'Set TEHAS_X.' >"$log"
}
fragment() { printf '%b' "$2" >"$frags/$1"; }

t_case "release-notes prints two fragments' lines under their headings, by issue number"
fresh
fragment 12.md '### Changes\n\n- from twelve\n  wrapped\n\n### Adopter action\n\nSet TEHAS_TWELVE.\n'
fragment 3.md '### Adopter action\n\nRun tehas update.\n\n### Changes\n\n- from three\n'
fragment my-branch.md '### Changes\n- from a branch\n'
t_run "'$notes' Unreleased '$log'"
t_eq "$t_rc" 0
t_eq "$t_out" "$(printf '%s\n' '### Changes' '' '- in the file' '- from three' '- from twelve' '  wrapped' \
  '- from a branch' '' '### Adopter action' '' 'Run tehas update.' '' 'Set TEHAS_TWELVE.')"

t_case "a tagged release's notes are the file's alone"
t_run "'$notes' v1.2.3 '$log'"
t_eq "$t_out" "$(printf '%s\n' '### Changes' '' '- old' '' '### Adopter action' '' 'Set TEHAS_X.')"

t_case "every release of the checkout's CHANGELOG.md reads the same beside fragments"
mkdir -p "$T/repo/changelog.d"
cp "$T_ROOT/CHANGELOG.md" "$T/repo/CHANGELOG.md"
printf '### Changes\n\n- a fragment\n\n### Adopter action\n\nDo it.\n' >"$T/repo/changelog.d/1.md"
same=0 differ=""
while read -r tag; do
  if [ "$("$notes" "$tag" - <"$T_ROOT/CHANGELOG.md")" = "$("$notes" "$tag" "$T/repo/CHANGELOG.md")" ]; then
    same=$((same + 1))
  else
    differ+=" $tag"
  fi
done < <(sed -n 's/^## \(v[0-9]*\.[0-9]*\.[0-9]*\).*/\1/p' "$T_ROOT/CHANGELOG.md")
t_eq "$differ" ""
[ "$same" -gt 0 ] || t_bad "the checkout's CHANGELOG.md has no release"

t_case "standard input has no fragments"
t_run "'$notes' Unreleased - <'$log'"
t_eq "$t_out" "$(printf '%s\n' '### Changes' '' '- in the file' '' '### Adopter action' '' 'None.')"

t_case "collecting rewrites the changelog, deletes the fragments and names them"
t_run "'$collect' '$log'"
t_eq "$t_rc" 0
t_eq "$t_out" "$(printf '%s\n' "$frags/3.md" "$frags/12.md" "$frags/my-branch.md")"
t_eq "$(ls -A "$T/log")" CHANGELOG.md
t_eq "$(cat "$log")" "$(printf '%s\n' '# Changelog' '' '## Unreleased' '' '### Changes' '' '- in the file' \
  '- from three' '- from twelve' '  wrapped' '- from a branch' '' '### Adopter action' '' 'Run tehas update.' '' \
  'Set TEHAS_TWELVE.' '' '## v1.2.3 (2026-10-21)' '' '### Changes' '' '- old' '' '### Adopter action' '' 'Set TEHAS_X.')"

t_case "with no fragments nothing changes"
before=$(cat "$log")
t_run "'$collect' '$log'"
t_eq "$t_rc" 0
t_eq "$t_out" ""
t_eq "$(cat "$log")" "$before"
t_run "'$notes' Unreleased '$log'"
t_eq "$t_rc" 0

t_case "a section without the headings gets them, and an action beside text keeps the text"
fresh
printf '%s\n' '# Changelog' '' '## Unreleased' '' '## v1.2.3 (2026-10-21)' '' '### Adopter action' '' 'Set TEHAS_X.' >"$log"
fragment 5.md '### Changes\n\n- five\n\n### Adopter action\n\nFive.\n'
t_run "'$collect' --print '$log'"
t_eq "$t_out" "$(printf '%s\n' '# Changelog' '' '## Unreleased' '' '### Changes' '' '- five' '' '### Adopter action' '' 'Five.' '' \
  '## v1.2.3 (2026-10-21)' '' '### Adopter action' '' 'Set TEHAS_X.')"
t_eq "$(ls "$frags")" 5.md
t_run "'$collect' --print --section v1.2.3 '$log'"
t_eq "$t_out" "$(printf '%s\n' '# Changelog' '' '## Unreleased' '' '## v1.2.3 (2026-10-21)' '' '### Changes' '' '- five' '' \
  '### Adopter action' '' 'Set TEHAS_X.' '' 'Five.')"

t_case "a fragment with text outside its two headings is refused, and nothing changes"
fresh
before=$(cat "$log")
fragment 7.md 'A line.\n\n### Changes\n\n- seven\n'
t_run "'$collect' '$log'"
t_eq "$t_rc" 1
t_contains "$t_out" "7.md line 1: text outside"
t_eq "$(cat "$log")" "$before"
t_eq "$(ls "$frags")" 7.md
fragment 7.md '### Changes\n\n- seven\n\n### Notes\n\n- no\n'
t_run "'$collect' '$log'"
t_eq "$t_rc" 1
t_contains "$t_out" "7.md line 5"
t_run "'$notes' Unreleased '$log'"
t_eq "$t_rc" 1
t_contains "$t_out" "release-notes: "
t_contains "$t_out" "text outside"

t_case "a fenced code block in a fragment is text, so a # line in it is not a heading"
fresh
# shellcheck disable=SC2016  # the backticks are a code fence
fragment 9.md '### Adopter action\n\nRun:\n\n```sh\n# as the tehas user\ntehas update\n```\n'
t_run "'$notes' Unreleased '$log'"
t_eq "$t_rc" 0
t_eq "$t_out" "$(printf '%s\n' '### Changes' '' '- in the file' '' '### Adopter action' '' 'Run:' '' '```sh' \
  '# as the tehas user' 'tehas update' '```')"
# shellcheck disable=SC2016  # the backticks are a code fence
fragment 9.md '### Changes\n\n```\n## Not a release\n```\n'
t_run "'$collect' '$log'"
t_eq "$t_rc" 1
t_contains "$t_out" "9.md line 4: a line of the code block from line 3 starts like a heading of CHANGELOG.md"
fragment 9.md '### Changes\n\n```\n# never closed\n'
t_run "'$collect' '$log'"
t_eq "$t_rc" 1
t_contains "$t_out" "9.md line 3: the code block is not closed"

t_case "a branch with no issue names its fragment with each / a -"
t_run "'$collect' --name fix/typo"
t_eq "$t_rc" 0
t_eq "$t_out" changelog.d/fix-typo.md
t_run "'$collect' --name bl/a/b"
t_eq "$t_out" changelog.d/bl-a-b.md
t_run "'$collect' --name ''"
t_eq "$t_rc" 64

t_case "a section that is not there is 1, a bad argument 64"
fresh
fragment 8.md '### Changes\n\n- eight\n'
t_run "'$collect' --section v9.9.9 '$log'"
t_eq "$t_rc" 1
t_contains "$t_out" "no section for v9.9.9"
t_eq "$(ls "$frags")" 8.md
t_run "'$collect' --bogus"
t_eq "$t_rc" 64
t_run "'$collect' --section"
t_eq "$t_rc" 64
t_run "'$collect' '$log' '$log'"
t_eq "$t_rc" 64
t_run "'$collect' '$T/no-such-file'"
t_eq "$t_rc" 1

t_case "the checkout's changelog fragments are well formed"
t_run "'$collect' --print '$T_ROOT/CHANGELOG.md' >/dev/null"
t_eq "$t_rc" 0
t_eq "$t_out" ""

t_done
