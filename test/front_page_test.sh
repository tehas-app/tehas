#!/usr/bin/env bash
# Tests for the front page, the plain dashboard: what the header band is when
# the window is phone sized, and how its islands set what a file says.
# It reads the markup, the stylesheets and the scripts as they are in the
# checkout: no browser, no server, no rendering.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

sheet=$T_ROOT/host/fun/manual.css
manual=$T_ROOT/host/docs/operating.md
features=$T_ROOT/host/docs/features.md
frag=$T_ROOT/host/docs/dashboard.frag
factory=$T_ROOT/host/fun/factory
manual_text=$(cat "$manual")
features_text=$(cat "$features")
frag_text=$(cat "$frag")
dashjs_text=$(cat "$factory"/*.js)

# The body of one media block: the first block in the file that opens with
# this, from its opening line to the line that closes it, braces counted.
media() { # <opens with this>
  awk -v open="$1" '
    !got && index($0, open) { got = 1 }
    got {
      print
      depth += gsub(/{/, "{") - gsub(/}/, "}")
      if (depth == 0) exit
    }
  ' "$sheet"
}
phone=$(media '@media (max-width: 1279px) {')
phone_small=$(media '@media (max-width: 699px) {')

# A width, or NO_WIDTH where the file does not hold the declaration, so that an
# absent declaration cannot read as a zero a panel would fit into.
NO_WIDTH="no width"
fits() { # <needs this many px> <has this, or no width>
  case "$2" in
    *[!0-9]*) echo "$NO_WIDTH" ;;
    *) [ "$1" -le "$2" ] && echo fits || echo "too wide" ;;
  esac
}

# The links of a page's section menu, in markup order, one per line.
nav_labels() { # <page file>
  awk -v open='<nav class="menu"' -v shut='</nav>' '
    !on && index($0, open) { on = 1; next }
    on && index($0, shut) { exit }
    on {
      rest = $0
      while ((p = index(rest, "<a ")) > 0) {
        rest = substr(rest, p)
        gt = index(rest, ">")
        cl = index(rest, "</a>")
        if (!gt || !cl) break
        label = substr(rest, gt + 1, cl - gt - 1)
        if (label != "") print label
        rest = substr(rest, cl + 4)
      }
    }
  ' "$1"
}

# How many rows the menu links need in a row of this many pixels. Topaz
# advances half an em, 8px a character at body size; a link is 24px of type
# with 12px of padding above and below, so a row of links is 48px. flex-wrap
# fills a row before it starts the next, so this counts the greedy way.
menu_rows() { # <labels> <row holds this many px> <gap between links>
  awk -v avail="$2" -v gap="$3" '
    { wide = length($0) * 8 }
    !used { used = wide; next }
    used + gap + wide <= avail { used += gap + wide; next }
    { rows++; used = wide }
    END { print rows + 1 }
  ' <<<"$1"
}

# The height of the header band: its padding, the row the sprite makes, the
# rows of links, the 4px rule under the band and the 8px Brass stripe painted
# below that.
band() { # <padding px> <wordmark row px> <rows of links>
  echo $(($1 + $2 + $3 * 48 + 4 + 8))
}

# The layout runs no script for its header, so its clearance is the plain
# literal of the band. Its menu keeps the one row that scrolls, so raising it
# would put the literal above the band the page really renders.
t_case "the short headers keep one row of links and the clearance they are given"
short=$(nav_labels "$T_ROOT/host/docs/_layout.html")
t_eq "$(awk '/^ *\.topbar-short/ && /flex-wrap/ { n++ } END { print n + 0 }' <<<"$phone")" 0
t_contains "$phone" '.topbar-short > .menu { overflow-x: auto; scrollbar-width: none; }'
t_eq "$(menu_rows "$short" 652 16)" 1
t_eq "$(menu_rows "$short" 343 16)" 1
t_contains "$phone" ':root { --header: 128px; }'
t_contains "$phone_small" ':root { --header: 120px; }'
t_eq "$(fits "$(band 16 50 1)" 128)" fits
t_eq "$(fits "$(band 8 40 1)" 120)" fits

t_case "the factory floor left core in 2.0: no page, stylesheet or script of it"
for gone in index.html game-view.js game-view.css game-data.js brick-wall.js fold-fit.js sample-issues.js theme.js stages.js; do
  [ ! -e "$T_ROOT/host/fun/$gone" ] || t_bad "host/fun/$gone is still there"
done
t_lacks "$(cat "$T_ROOT/host/docs/_layout.html")" 'game-view'
t_lacks "$frag_text" 'game-view'

t_case "what a file says reaches the plain dashboard as text, never as markup"
for text in "$frag_text" "$dashjs_text"; do
  t_lacks "$text" 'innerHTML'
  t_lacks "$text" 'insertAdjacentHTML'
  t_lacks "$text" 'outerHTML'
done
t_lacks "$frag_text" '<script type="module">'
t_lacks "$dashjs_text" 'insertRow'
# What the plain dashboard polls: the queue every ten seconds, and the console
# of a cycle every two.
t_contains "$(cat "$factory/tehas-factory-state.render.js")" 'feed(props.src, { every: 10000 })'
t_contains "$(cat "$factory/tehas-factory-tail.js")" 'const EVERY = 2000;'

t_case "a paused factory is said as paused, and as finishing the issue it has"
t_contains "$dashjs_text" 'if (s.paused) pill = ["pause", finishing.length ? "paused, finishing #"'
t_lacks "$dashjs_text" 'else if (s.paused) pill = ["pause", "paused"];'
t_contains "$manual_text" 'paused and, while a cycle is still running, that it is finishing that issue'
t_contains "$features_text" 'not proven that a cycle finishes while the factory is paused'

t_done
