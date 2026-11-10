#!/usr/bin/env bash
# Tests for the manual in host/docs/ and README.md: every link that names a
# section lands on the page that holds it, the commands page and the pages
# split from it together hold each `## ` heading once, and nothing says that
# libexec/ is all bash or that a cycle's container is built by libexec/work.
# shellcheck disable=SC2016  # the patterns are literals the cases match with
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

docs=$T_ROOT/host/docs

# The anchor the docs server makes from a heading, over the heading's text with
# its leading hashes already off: backticks and asterisks go, the rest comes to
# lower case, a character that is no word, space or dash goes, and a space
# becomes a dash.
slug_of_heading() {
  sed -e 's/[`*]//g' -e 'y/ABCDEFGHIJKLMNOPQRSTUVWXYZ/abcdefghijklmnopqrstuvwxyz/' \
    -e 's/[^a-z0-9 _-]//g' -e 'y/ /-/'
}

# The headings of <file> as anchors, one line per heading, sorted and alone:
# every line after # to ###### that is outside a fenced code block. A fence is a
# line that starts with three backticks, and a heading inside one is text.
anchors_of() { # <file>
  awk '
    /^```/ { fence = !fence; next }
    fence { next }
    /^#{1,6} / { line = $0; sub(/^#+ /, "", line); print line }
  ' "$1" | slug_of_heading | LC_ALL=C sort -u
}

# The lines of <file> that name a section, one per line with its hashes off:
# the same lines anchors_of reads, before they become anchors.
headings_of() { # <file>
  awk '
    /^```/ { fence = !fence; next }
    fence { next }
    /^#{1,6} / { line = $0; sub(/^#+ /, "", line); print line }
  ' "$1"
}

# The `## ` headings of <file> as anchors, in the order they stand: the count
# the split is measured by.
h2_of() { # <file>
  awk '
    /^```/ { fence = !fence; next }
    fence { next }
    /^## [^#]/ { line = $0; sub(/^## /, "", line); print line }
  ' "$1" | slug_of_heading
}

# What a link names: a page and an anchor, tab separated, the page empty for an
# in-page link. The two shapes are ](#anchor) and ](/page#anchor); the pattern
# is given to grep with -- so its parentheses are its own.
links_of() { # <file>
  grep -oE '\]\((/[a-z0-9-]+)?#[a-z0-9-]+\)' -- "$1" |
    while IFS= read -r l; do
      body=${l#']('}
      body=${body%')'}
      page=${body%%'#'*}
      anchor=${body#*'#'}
      printf '%s\t%s\n' "${page#/}" "$anchor"
    done
}

# The pages the two link rules cover: the manual and the repository's own README.
pages=("$docs"/*.md "$T_ROOT/README.md")

t_case "every link to a section names a heading of the page it names"
bad=""
for f in "${pages[@]}"; do
  name=${f##*/}
  self=${name%.md}
  [ "$name" = README.md ] && self=README
  while IFS=$'\t' read -r page anchor; do
    [ -n "$anchor" ] || continue
    [ -n "$page" ] || page=$self
    if [ "$page" = README ]; then
      target=$T_ROOT/README.md
    else
      target=$docs/$page.md
    fi
    if [ ! -f "$target" ]; then
      bad+="$name: no page named /$page for the link to #$anchor"$'\n'
      continue
    fi
    anchors_of "$target" | grep -qxF -- "$anchor" ||
      bad+="$name: #$anchor is no heading of the page $page"$'\n'
  done < <(links_of "$f")
done
t_eq "$bad" ""

# The commands page and the pages split from it, one list: the count of the
# pages and the count of their headings are both read from what is on disk.
tool_pages=("$docs/tools.md" "$docs"/tools-*.md)

t_case "the commands page and fourteen pages by tool, each with front matter"
t_eq "${#tool_pages[@]}" 15
for f in "${tool_pages[@]}"; do
  [ -f "$f" ] || { t_bad "$f is missing"; continue; }
  head -n 4 "$f" | grep -q '^title: ' ||
    t_bad "${f##*/} has no title in its front matter"
  head -n 4 "$f" | grep -q '^description: ' ||
    t_bad "${f##*/} has no description in its front matter"
done

t_case "the tools pages together hold each of the 44 headings once"
# The 44 are the `## ` headings the commands page held before the split, named
# here once. A heading that moved to a page of its own is still one heading;
# one written out twice is two pages claiming the same section.
want=(
  instance-settings tehas-dispatch tehas-work tehas-plan tehasbot-review
  tehas-dev tehas-index tehas-setup tehas-labels tehas-check-product
  tehas-init-product tehas-limit tehas-drain tehas-history tehas-export
  tehas-state tehas-pause tehas-resume tehas-stop tehas-status tehas-api
  tehas-disk tehas-top tehas-analytics tehasbot tehasbot-cycle
  tehasbot-status-and-the-console tehasbot-health tehasbot-energy tehasbot-top
  tehasbot-protolab tehasbot-improve tehasbot-refresh tehasbot-github-token
  tehasbot-github-writes tehasbot-clean tehasbot-preview
  the-gates-run-by-the-cycle-usable-by-hand tehas-update installsh
  releases polyfills-of-pi-rukas-features bootstrapsh scriptstestsh
)
t_eq "${#want[@]}" 44
have=$(
  for f in "${tool_pages[@]}"; do h2_of "$f"; done | LC_ALL=C sort
)
t_eq "$(printf '%s\n' "$have" | wc -l)" 44
t_eq "$(uniq -d <<<"$have" | tr '\n' ' ')" ""
t_eq "$(comm -13 <(printf '%s\n' "${want[@]}" | LC_ALL=C sort -u) <(printf '%s\n' "$have" | LC_ALL=C sort -u) | tr '\n' ' ')" ""
t_eq "$(comm -23 <(printf '%s\n' "${want[@]}" | LC_ALL=C sort -u) <(printf '%s\n' "$have" | LC_ALL=C sort -u) | tr '\n' ' ')" ""

t_case "tools.md keeps the list, the settings, the gates and the test script"
for h in instance-settings the-gates-run-by-the-cycle-usable-by-hand scriptstestsh; do
  h2_of "$docs/tools.md" | grep -qxF -- "$h" || t_bad "tools.md no longer holds ## ${h//-/ }"
done
# And names every page of the set, so a page that went missing is named by
# nothing and the case above finds a heading gone with it.
for f in "${tool_pages[@]}"; do
  name=${f##*/}
  name=${name%.md}
  [ "$name" = tools ] && continue
  grep -qF -- "](/$name" "$docs/tools.md" || t_bad "tools.md lists no page for /$name"
done

t_case "no manual page says libexec is all bash or names libexec/work"
bad=""
for f in "${pages[@]}"; do
  grep -qiE 'libexec[^|]*all bash|all bash[^|]*libexec' "$f" &&
    bad+="${f##*/}: says libexec is all bash"$'\n'
  grep -qF -- 'libexec/work' "$f" &&
    bad+="${f##*/}: names libexec/work"$'\n'
done
t_eq "$bad" ""

t_case "the pages say what libexec holds"
# In the repository three bash scripts; installed, also the console and the
# Rust programs install.sh builds; every other command a verb of the console.
t_contains "$(cat "$T_ROOT/README.md")" '`setup` and `update`'
t_contains "$(cat "$T_ROOT/README.md")" 'install.sh` builds'
t_contains "$(cat "$docs/host.md")" 'Rust programs `install.sh` builds'
t_contains "$(cat "$docs/tools.md")" 'three bash scripts'
t_contains "$(cat "$docs/features.md")" 'verb of the console'

t_case "every heading of a tools page is written once on that page"
# Two pages of the manual share a word (`## tehas api` is on the commands page
# and on the factory API page, and a link from either means its own page), so
# the rule is within one page: a heading twice on one page is one anchor for two
# sections, and the links above it land on the first.
bad=""
for f in "${pages[@]}"; do
  [ -f "$f" ] || continue
  dup=$(headings_of "$f" | LC_ALL=C sort | uniq -d)
  [ -z "$dup" ] || bad+="${f##*/}: $(tr '\n' ' ' <<<"$dup")"
done
t_eq "$bad" ""

t_done
