#!/usr/bin/env bash
# Tests for the manual: what it says about libexec/, and the anchors it links
# to. A statement about what a directory holds goes false when a program moves
# into or out of it, and an anchor whose heading is renamed or moved lands a
# reader on the top of a page.
# shellcheck disable=SC2016  # the slug rule and the greps are literal text
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

docs="$T_ROOT/host/docs"
pages=("$docs"/*.md "$T_ROOT/README.md")

# The greps below read the pages by name, each of them a file this file checks.
# An empty expansion would leave a pattern read from standard input, and a case
# with no page in front of it would pass by finding nothing.
for page in "${pages[@]}"; do
  [ -f "$page" ] || t_bad "not a page: $page"
done

# t_slug <heading>: the anchor the docs server gives a heading. Caddy renders
# the page with its markdown directive, whose rule is: lowercase, spaces to
# dashes, and no other punctuation. `## scripts/test.sh` is #scriptstestsh and
# `## bootstrap.sh` is #bootstrapsh.
t_slug() {
  local text=$1
  text=${text,,}
  text=${text// /-}
  printf '%s\n' "${text//[^a-z0-9-]/}"
}

# The rule checked against the headings of the manual before it is used to
# judge one: a rule that matches nothing would pass every case below.
t_case "the slug rule gives the anchors the manual already links"
t_eq "$(t_slug 'tehasbot health')" "tehasbot-health"
t_eq "$(t_slug 'Sandbox image')" "sandbox-image"
t_eq "$(t_slug 'scripts/test.sh')" "scriptstestsh"
t_eq "$(t_slug 'bootstrap.sh')" "bootstrapsh"

t_case "no line that names libexec says all bash"
hits=$(grep -in 'libexec' "${pages[@]}" | grep -i 'all bash' || true)
[ -z "$hits" ] || t_bad "calls libexec all bash: $hits"

t_case "no page names libexec/work"
hits=$(grep -rn 'libexec/work' "${pages[@]}" 2>/dev/null || true)
[ -z "$hits" ] || t_bad "names a command that is not installed: $hits"

t_case "no page links the two dead anchors"
hits=$(grep -rn '#the-health-file\|#the-sandbox-build' "${pages[@]}" 2>/dev/null || true)
[ -z "$hits" ] || t_bad "links a heading that is not there: $hits"

# The two headings the anchors were pointed at, each in the page that holds its
# text: the health file in tools.md, the build that keeps the newest two in
# host.md.
# The headings the two anchors name, each read from the page whose "## " lines
# really hold it: a `sed` of one fixed line would say the same whether or not
# the heading stands in that page.
headings=$(sed -n 's/^## \(.*\)$/\1/p' "$docs/tools.md" "$docs/host.md")
[ "$(printf '%s\n' "$headings" | grep -cxF 'tehasbot health')" = 1 ] ||
  t_bad "no ## tehasbot health among the headings of the two pages"
[ "$(printf '%s\n' "$headings" | grep -cxF 'Sandbox image')" = 1 ] ||
  t_bad "no ## Sandbox image among the headings of the two pages"

t_case "tools.md has the ## heading the health anchor names"
t_eq "$(t_slug "$(sed -n 's/^## \(tehasbot health\)$/\1/p' "$docs/tools.md")")" \
  "tehasbot-health"
t_eq "$(t_slug "$(sed -n 's/^## \(tehasbot health\)$/\1/p' "$docs/tools.md")")" \
  "tehasbot-health"

t_case "host.md has the ## heading the sandbox anchor names"
t_eq "$(t_slug "$(sed -n 's/^## \(Sandbox image\)$/\1/p' "$docs/host.md")")" \
  "sandbox-image"

# The count the page had when the anchors were repointed. A section moved out of
# tools.md would leave the commands it documents where nobody looks for them.
t_case "tools.md holds the sections it held on the base commit"
t_eq "$(grep -c '^## ' "$docs/tools.md")" 44

t_done
