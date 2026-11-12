#!/usr/bin/env bash
# Tests for the manual: what it says about libexec/, the anchors it links to,
# and the pages the commands are split over. A statement about what a directory
# holds goes false when a program moves into or out of it, and an anchor whose
# heading is renamed or moved lands a reader on the top of a page.
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

# The two headings the anchors name, each read from the page whose "## " lines
# really hold it: the health file in the page that took the health section, the
# build that keeps the newest two in host.md.
headings=$(sed -n 's/^## \(.*\)$/\1/p' "$docs/tools-health.md" "$docs/host.md")
[ "$(printf '%s\n' "$headings" | grep -cxF 'tehasbot health')" = 1 ] ||
  t_bad "no ## tehasbot health among the headings of the two pages"
[ "$(printf '%s\n' "$headings" | grep -cxF 'Sandbox image')" = 1 ] ||
  t_bad "no ## Sandbox image among the headings of the two pages"

t_case "the health page has the ## heading the health anchor names"
t_eq "$(t_slug "$(sed -n 's/^## \(tehasbot health\)$/\1/p' "$docs/tools-health.md")")" \
  "tehasbot-health"

t_case "host.md has the ## heading the sandbox anchor names"
t_eq "$(t_slug "$(sed -n 's/^## \(Sandbox image\)$/\1/p' "$docs/host.md")")" \
  "sandbox-image"

# The pages the commands are split over. Named here so a case that finds
# nothing in one of them fails rather than passing by looking at nothing.
TOOLS_PAGES=("$docs"/tools.md "$docs"/tools-dispatch.md "$docs"/tools-work.md \
  "$docs"/tools-plan.md "$docs"/tools-dev.md "$docs"/tools-setup.md \
  "$docs"/tools-control.md "$docs"/tools-status.md "$docs"/tools-tehasbot.md \
  "$docs"/tools-health.md "$docs"/tools-protolab.md "$docs"/tools-github.md \
  "$docs"/tools-clean.md "$docs"/tools-update.md "$docs"/tools-releases.md)
for page in "${TOOLS_PAGES[@]}"; do
  [ -f "$page" ] || t_bad "not a page: $page"
done

# t_anchors <page> <file>: the anchors of a page, written to <file>. Its `## `
# and `### ` headings, and the lead-in of a paragraph that is nothing but bold,
# which the markdown directive turns into a heading of the rendered page as
# well. The list goes to a file and not down a pipe: grep closes its standard
# input as soon as it matches, and with pipefail set the writer's signal reads
# as a failure of the case.
t_anchors() {
  sed -n 's/^#\{2,3\} //p; s/^\*\*\(.\{1,\}\)\.\*\*.*/\1/p' "$1" \
    | while IFS= read -r text; do t_slug "$text"; done > "$2"
}

# The anchors of a page, checked against the rule before they are used to
# judge a link: a page with no heading would answer every link with nothing
# and pass.
t_case "the anchor rule finds the headings of a tools page"
t_anchors "$docs/tools-health.md" "$T/anchors"
t_eq "$(grep -c . "$T/anchors")" 5
t_anchors "$docs/tools-tehasbot.md" "$T/anchors"
grep -qxF -- the-pull-request-of-a-stopped-cycle "$T/anchors" \
  || t_bad "the bold lead-in of the finish section is no anchor"

# A link to a section of the commands page names the page that holds it now, so
# the page is the file of that name and the anchor is the field after the `#`.
# Every such link of the manual and of the README has to name a heading of the
# page it names. tools.md is one of the pages: it keeps the settings, the gates
# and scripts/test.sh.
t_links_of() {
  grep -oE '\]\(/tools(-[a-z]+)?#[a-z0-9-]+\)' "$1" | sed 's/^](\///; s/)$//'
}

t_case "every link to a tools page names a heading of that page"
for from in "${pages[@]}"; do
  while IFS= read -r link; do
    [ -n "$link" ] || continue
    file=${link%%#*}
    anchor=${link#*#}
    [ -e "$docs/$file.md" ] || { echo "$from: no page $file.md"; continue; }
    [ "$anchor" = "$link" ] && continue      # a link with no anchor
    # The anchors go to a file rather than down a pipe, as t_anchors says.
    t_anchors "$docs/$file.md" "$T/anchors"
    grep -qxF -- "$anchor" "$T/anchors" \
      || echo "$from: ](/$link) names no heading of $file.md"
  done < <(t_links_of "$from" | sort -u)
done > "$T/tmp-links" 2>&1
t_eq "$(wc -l < "$T/tmp-links" | tr -d ' ')" 0

# Within a page, `](#anchor)` is a promise about that page alone: a section
# that moved and left its links behind breaks here.
t_same_page_links_of() {
  grep -oE '\]\(#[a-z0-9-]+\)' "$1" | sed 's/^](#//; s/)$//'
}

t_case "every same-page link of a tools page names a heading of that page"
for page in "${TOOLS_PAGES[@]}"; do
  while IFS= read -r anchor; do
    [ -n "$anchor" ] || continue
    t_anchors "$page" "$T/anchors"
    grep -qxF -- "$anchor" "$T/anchors" \
      || echo "$(basename "$page"): no heading for ](#$anchor)"
  done < <(t_same_page_links_of "$page" | sort -u)
done > "$T/tmp-same" 2>&1
t_eq "$(wc -l < "$T/tmp-same" | tr -d ' ')" 0

# The pages of the split carry what every page of the manual carries: a title
# for the layout and a description for a reader who finds only the page.
t_case "each tools page has its title and description"
for page in "${TOOLS_PAGES[@]}"; do
  head -n 4 "$page" | grep -q '^title: ' \
    || t_bad "$(basename "$page") has no title"
  head -n 4 "$page" | grep -q '^description: "' \
    || t_bad "$(basename "$page") has no description"
done

# The set of sections, rather than the count of one page: the pages together
# hold the 44 `## ` sections tools.md held on the base commit, each once. The
# one heading the split adds is `## The pages`, the list the commands page
# gives of them. A section left in two pages, or dropped with the page it was
# moved out of, shows here.
t_case "the tools pages hold the 44 sections of tools.md, each once"
t_eq "$(cat "${TOOLS_PAGES[@]}" | grep -c '^## ')" 45
t_eq "$(cat "${TOOLS_PAGES[@]}" | grep '^## ' | grep -v '^## The pages$' \
   | sort | uniq -d | wc -l | tr -d ' ')" 0

t_done
