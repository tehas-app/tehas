#!/usr/bin/env bash
# Tests for the front page: how the panels take the width of the window, what
# the header band is when the window is phone sized, and what answers a ticket
# row that is pressed.
# It reads the markup, the stylesheets and the scripts that draw into the
# markup as they are in the checkout: no browser, no server, no rendering.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

page=$T_ROOT/host/fun/index.html
game=$T_ROOT/host/fun/game-view.css
board=$T_ROOT/host/fun/game-view.js
manual=$T_ROOT/host/docs/operating.md
features=$T_ROOT/host/docs/features.md
frag=$T_ROOT/host/docs/dashboard.frag
factory=$T_ROOT/host/fun/factory
manual_text=$(cat "$manual")
features_text=$(cat "$features")
frag_text=$(cat "$frag")
dashjs_text=$(cat "$factory"/*.js)
page_text=$(cat "$page")
game_text=$(cat "$game")
board_text=$(cat "$board")

# The markup of one element: from the line that opens it to the line that
# closes it. Each of these holds no element of its own kind, so the first line
# that closes the tag is the element's own.
block() { # <opens with> <closes with>
  awk -v o="$1" -v c="$2" '!on && index($0, o) { on = 1 } on { print } on && index($0, c) { exit }' "$page"
}
head_panel=$(block '<div class="floor-head">' '</div>')
topbar=$(block '<header class="topbar">' '</header>')
doors=$(block '<nav class="doors"' '</nav>')
footer_band=$(block '<footer class="footer">' '</footer>')
floor_intro=$(block '<p class="floor-intro">' '</p>')

# The deep link into the manual, named once: the target and the words of it, and
# the heading of the manual section they open. The slug in the target is generated
# by the docs server from that heading, so the case derives it from the heading
# rather than trusting a copy: a renumbered or reworded section changes it there.
how_to_write='/operating#1-write-an-issue-tehas-can-build'
how_to_write_label='HOW TO WRITE AN ISSUE'
manual_heading='## 1. Write an issue tehas can build'

# The media query a line of a stylesheet sits in: the last @media above it, and
# of the lines that hold the pattern, the first one. NO_QUERY when the line is
# in the file with no media query around it, and "not in the file" when the
# line is not there at all, so that a changed value cannot read as the answer for a line
# outside every media query.
query_of() { # <holds this> <file>
  awk -v p="$1" '/@media/ { q = $0 } !seen && index($0, p) { seen = 1; found = q } END { print !seen ? "not in the file" : (found == "" ? "no media query" : found) }' "$2"
}
NO_QUERY="no media query" # a line of the file with no @media around it
NO_SLUG="no slug" # the manual holds no heading to derive the deep link's anchor from
NO_ORDER="no link" # one of the two links of the intro is not on the page at all

# The anchor the docs server makes from a heading: lower case, spaces for hyphens,
# the punctuation of the number gone. Read from its standard input, so a caller
# that has the text at hand (a file, a variable) can say where the heading lives.
# NO_SLUG where the input holds no such heading, so an absent one cannot read as
# the empty anchor of a page that opens at its top.
slug_of() { # <heading>
  awk -v h="$1" -v no="$NO_SLUG" '$0 == h {
      slug = tolower(h)
      sub(/^## /, "", slug)
      gsub(/[^a-z0-9 ]/, "", slug)
      gsub(/ /, "-", slug)
      found = 1
    }
    END { print found ? slug : no }'
}

# A width, or NO_WIDTH where the file does not hold the declaration, so that an
# absent declaration cannot read as a zero a panel would fit into.
NO_WIDTH="no width"
fits() { # <needs this many px> <has this, or no width>
  case "$2" in
    *[!0-9]*) echo "$NO_WIDTH" ;;
    *) [ "$1" -le "$2" ] && echo fits || echo "too wide" ;;
  esac
}

# The media block whose .stage rule drops the monitor's 336px column: the band
# where the stage becomes one column, and the width it names. NO_BAND and NO_WIDTH
# where no block does, so an absent band cannot read as a width of its own.
NO_BAND="no band"
stage_band=$(awk -v no="$NO_BAND" '
  /^@media/ { q = $0 }
  /^  \.stage \{/ { instage = 1 }
  instage && !got && /grid-template-columns: minmax\(0, 1fr\);/ { got = q }
  instage && /^  \}/ { instage = 0 }
  END { print got == "" ? no : got }' "$game")
stage_from=$(awk -v no="$NO_WIDTH" 'match($0, /min-width: [0-9]+/) {
    print substr($0, RSTART + 11, RLENGTH - 11); found = 1
  } END { if (!found) print no }' <<<"$stage_band")

# The row a panel claims: the area its own rule names it. NO_AREA where none
# does, which is the shape that lets the grid move a panel.
NO_AREA="no area"
area_of() { # <class>
  awk -v sel=".$1 " -v no="$NO_AREA" '
    index($0, sel) == 1 || index($0, sel) == 3 { on = 1 }
    on && match($0, /grid-area: [a-z-]+/) {
      print substr($0, RSTART + 11, RLENGTH - 11); found = 1; exit
    }
    on && index($0, "}") { on = 0 }
    END { if (!found) print no }' "$game"
}

# Every rule that opens with this selector and holds this declaration, printed
# whole. A stylesheet names a selector more than once; only one of those rules
# draws the face. The page's own stylesheet unless another is named.
rules_holding() { # <opens with this> <declaration line holds this> [in this file]
  awk -v sel="$1" -v want="$2" '
    index($0, sel) == 1 || index($0, sel) == 3 { on = 1; body = "" }
    on { body = body $0 "\n" }
    on && index($0, "}") {
      on = 0
      if (index(body, want)) printf "%s", body
    }
  ' "${3:-$game}"
}

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
  ' "$game"
}
phone=$(media '@media (max-width: 1279px) {')
phone_small=$(media '@media (max-width: 699px) {')
medium=$(media "$stage_band") # the band where the stage gives up the monitor column
# A block without its @media line, which names a width of its own.
medium_body=$(awk 'NR > 1' <<<"$medium")
phone_body=$(awk 'NR > 1' <<<"$phone")

# The first rule that opens with this, at the start of its line or after the
# two spaces of a media block: the rule and its declarations, to the line that
# closes it. A grouped selector such as .station, .door, .monitor { is not it.
rule() { # <opening line starts with this>
  awk -v hold="$1" '!on && (index($0, hold) == 1 || index($0, hold) == 3) { on = 1 }
    on { print } on && index($0, "}") { exit }' "$game"
}

# What :root:has(.desk) sets the header clearance to inside a media block.
desk_clearance() { # <media block>
  awk '/:root:has\(.desk\)/ { match($0, /--header: [0-9]+px/); print substr($0, RSTART, RLENGTH) }' <<<"$1"
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

# The height of the header band: its padding, the row the sprite and the state
# tag make, the rows of links, the 4px rule under the band and the 8px Brass
# stripe painted below that.
band() { # <padding px> <wordmark row px> <rows of links>
  echo $(($1 + $2 + $3 * 48 + 4 + 8))
}

labels=$(nav_labels "$page")

# The work order is a panel in the column below 1440px, so nothing in the desk
# travels under the header while the page scrolls. The monitor beside the
# stations is the one that sticks, and it is outside every media block.
t_case "the one-column layouts pin nothing, and the wide monitor still sticks"
t_lacks "$medium" 'position: sticky'
t_lacks "$phone" 'position: sticky'
t_contains "$(rule '.monitor { position: static')" 'position: static;'
t_eq "$(query_of '.monitor {' "$game")" "$NO_QUERY"
wide=$(rules_holding '.monitor {' 'position: sticky;')
t_contains "$wide" 'position: sticky;'
t_contains "$wide" 'top: calc(var(--header) + 16px);'

# Below the width that fits four stations beside the monitor the stage is one
# column: the monitor's 336px column is gone, so every panel takes the whole
# width and nothing in the band is held short of it.
t_case "the medium stage is one column and every panel fills it"
t_contains "$(rule '.stage {')" 'grid-template-columns: minmax(0, 1fr) 336px;'
t_contains "$(rule '.stage {')" '"line monitor"'
t_contains "$medium" 'grid-template-columns: minmax(0, 1fr);'
t_contains "$medium" '.monitor { position: static; display: block; min-height: 0; }'
t_lacks "$medium_body" 'max-width:'
t_lacks "$phone_body" 'max-width:'

# The caps the one-column stage used to carry are gone from the rules as well as
# from the media blocks. Each case names the rule it looked at.
t_case "no panel of the stage is capped short of its column"
t_lacks "$game_text" 'max-width: 720px'
t_lacks "$(rule '.sign {')" 'max-width'
t_lacks "$(rule '.doors {')" 'max-width'

# The rows the monitor column named go with it, so the one-column template names
# them in its own order. A panel that names no area is placed after every named
# one, below the breakroom.
t_case "every panel of the one-column stage names its row"
medium_areas=$(awk '/^  \.stage \{/,/^  \}/' <<<"$medium" | grep -o '"[a-z ]*"' | tr '\n' ' ')
t_eq "$medium_areas" '"floor" "doors" "desk" "console" "epics" "break" '
t_lacks "$medium_body" 'grid-area: auto'

# That list is only right while it matches the markup. These are the elements the
# page puts directly in .stage: where the stage is one column they are its items,
# and each has to name a row of the template above or the grid puts it after the
# panels that do.
t_case "the markup puts these panels directly in the stage"
stage_children=$(awk '
  /^  <main class="stage">/ { on = 1; next }
  on && /^  <\/main>/ { exit }
  on {
    line = $0
    before = depth
    first = "" # the class of the element this line opens and does not close
    while (match(line, /<\/?[a-zA-Z][a-zA-Z0-9-]*[^>]*>/)) {
      t = substr(line, RSTART, RLENGTH); line = substr(line, RSTART + RLENGTH)
      shut = substr(t, 2, 1) == "/"
      name = shut ? substr(t, 3) : substr(t, 2)
      sub(/[^a-zA-Z0-9-].*/, "", name)
      if (name == "img" || name == "input" || name == "br" || name == "hr" || name == "meta" || name == "link") continue
      if (shut) { depth--; continue }
      depth++
      if (first == "" && match(t, /class="[a-z-]+"/)) first = substr(t, RSTART + 7, RLENGTH - 8)
    }
    if (first != "" && depth == before + 1 && depth == 1) print first
  }' "$page")
t_eq "$(tr '\n' ' ' <<<"$stage_children")" "floor doors desk epics break-room "
t_case "every panel the markup puts in the stage has a row of its own"
for panel in $stage_children; do
  t_contains "$medium_areas" "\"$(area_of "$panel")\""
done

# The row of four is what the complaint asks to keep: only the width that cannot
# hold four signs turns each one sideways, scene beside text.
t_case "the row of four survives the medium band and stacks only below 1280"
t_eq "$(query_of '.stations { grid-template-columns: minmax(0, 1fr); gap: 24px; }' "$game")" \
  "@media (max-width: 1279px) {"
t_lacks "$medium_body" '.stations'
t_lacks "$medium_body" '.sign'

# Centring the first screen and giving the monitor its height is what fills the
# height the monitor column stands over, so the script's wide width is the width
# where the stylesheet gives the stage that column. Both numbers come from the
# place that names one, and are compared to each other: a band that moves in one
# file has to move in the other.
t_case "the script is wide exactly where the stage gains its monitor column"
t_contains "$board_text" 'const wide = window.matchMedia("(min-width: 1440px)");'
js_wide=$(awk -v no="$NO_WIDTH" '
  /const wide = window\.matchMedia/ {
    n++
    if (!got && match($0, /min-width: [0-9]+/)) got = substr($0, RSTART + 11, RLENGTH - 11)
  }
  END { if (n != 1 || got == "") { print no; exit } print got }' "$board")
t_eq "$stage_from" "$js_wide"
t_eq "$js_wide" 1440

# A row that scrolls with its scrollbar hidden left the links past the edge
# reachable only by a swipe nobody could see.
t_case "the phone menu wraps its links into rows"
t_eq "$(query_of '.topbar:not(.topbar-short) > .menu {' "$game")" "@media (max-width: 1279px) {"
t_contains "$(rule '.topbar:not(.topbar-short) > .menu {')" 'flex-wrap: wrap;'
t_eq "$(awk '/overflow-x|scrollbar-width|webkit-scrollbar/ && !/topbar-short/ { n++ } END { print n + 0 }' <<<"$phone")" 0

# Every link the page shows at 375px is in the flow: 8px of band padding each
# side and 8px more on the menu leave 343px for a row of links.
t_case "every phone menu link fits a row and no link is wider than one"
t_eq "$(menu_rows "$labels" 343 16)" 3
t_eq "$(menu_rows "$labels" 288 16)" "$(menu_rows "$labels" 343 16)"
t_eq "$(fits "$(awk 'BEGIN { most = 0 } { w = length($0) * 8; if (w > most) most = w } END { print most }' <<<"$labels")" 343)" fits

# The clearance the front page needs is the band its wrapped menu makes, and
# only this page is given it: :has(.desk) is true on the front page alone.
t_case "the front page clears the header its wrapped menu makes"
t_eq "$(desk_clearance "$phone")" "--header: $(band 16 50 "$(menu_rows "$labels" 652 16)")px"
t_eq "$(desk_clearance "$phone_small")" "--header: $(band 8 40 "$(menu_rows "$labels" 343 16)")px"

# The manual loads game-view.css and runs no script, so its
# clearance is the plain literal of the band. Its menu holds four links and
# keeps the one row that scrolls, so raising it would put the literal above the
# band the page really renders.
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

# The body of one function of the front page's script: from the line that opens
# it to the line that closes it at the same indent (two spaces).
js_fn() { # <opens with this>
  awk -v open="$1" '
    !got && index($0, open) == 3 { got = 1 }
    got { print } got && $0 == "  }" { exit }
  ' "$board"
}
select_fn=$(js_fn 'function select(number, origin) {')

# Below 1440px the work order is a panel in the column: it can be off screen
# when a row is pressed, and the flying number is skipped there. The press still
# has to answer where it landed, so the row gets the brass latch the counts use.
t_case "a pressed row latches, and before the flight is judged"
t_contains "$select_fn" 'replay(origin, "is-changed");'
t_eq "$(awk '!latch && /replay\(origin, "is-changed"\)/ { latch = NR }
  !cut && /if \(!changed \|\| !from \|\| calm\.matches \|\| !onScreen/ { cut = NR }
  END { print latch && cut && latch < cut ? "before" : "wrong" }' <<<"$select_fn")" before

t_case "the latch is the flash the stylesheet animates and the calm block turns off"
t_contains "$game_text" '.is-changed { animation: latch 320ms steps(2, end); }'
t_contains "$(media '@media (prefers-reduced-motion: reduce) {')" '.is-changed,'

t_case "the row is marked as well, which is the half a calm browser keeps"
t_contains "$select_fn" 'markCurrent();'
t_contains "$board_text" 'node.classList.toggle("is-current", current);'
t_contains "$board_text" 'node.setAttribute("aria-current", current ? "true" : "false");'
t_contains "$game_text" '.row.is-current { background: var(--accent-soft); }'
t_contains "$game_text" '.ticket:hover, .ticket.is-current { background: var(--accent-soft); }'

t_case "the answer stays where it was pressed: selecting moves nothing on the page"
t_lacks "$select_fn" 'scrollIntoView'
t_lacks "$select_fn" 'scrollTo'
t_lacks "$select_fn" 'window.scroll'

# How a block element opens and closes across the whole page. A check that reads
# only ids and attributes passes with a closing tag missing, and one missing
# </div> moves everything after it inside the element before it: under
# .floor { display: contents } that empties the grid area of the factory line.
unbalanced=""
for tag in div section main header nav table thead tbody tr td th ul ol li figure p h1 h2 h3 dl dt dd span b i a button canvas; do
  o=$(grep -o "<${tag}[ >/]" <<<"$page_text" | wc -l)
  c=$(grep -o "</$tag>" <<<"$page_text" | wc -l)
  [ "$o" = "$c" ] || unbalanced="$unbalanced $tag($o/$c)"
done

t_case "the page closes every element it opens"
t_eq "$unbalanced" ""
# The head panel ends at its own closing tag: with that tag gone the block below
# it would be read as part of the panel.
t_contains "$head_panel" 'id="floor-summary"'
t_lacks "$head_panel" '<div class="line">'
t_lacks "$head_panel" 'class="stations"'

# The way into the manual's page on writing an issue, beside the way into the
# page on how the factory works. It is one link of the head panel's own text and
# takes the style .floor-intro a already gives a link, so it belongs nowhere else
# on the page.
t_case "the head panel asks a person how to write an issue"
# The words a person reads, and the heading they open: t_lacks cannot fail on an
# empty string, so each band is shown to be there before it is negated over.
t_contains "$manual_text" "$manual_heading"
# The anchor of the target, read out of the href itself and compared with the
# anchor the docs server makes from the manual's own heading: reword the heading
# and the two differ, which is the dead link this case exists to catch.
href_anchor=${how_to_write##*#}
t_eq "$(slug_of "$manual_heading" <<<"$manual_text")" "$href_anchor"
t_contains "$floor_intro" "<p class=\"floor-intro\">"
t_contains "$head_panel" "<a href=\"$how_to_write\">$how_to_write_label</a>"
t_contains "$floor_intro" "<a href=\"$how_to_write\">$how_to_write_label</a>"
# After the way into how the factory works. Both links are on the page's one
# line, so the needle's offset in that line is the order, read over the head
# panel the link belongs to. NO_ORDER when either link is gone, so an absent
# link is not read as a reordered one.
t_eq "$(awk -v a='>HOW IT WORKS</a>' -v b=">$how_to_write_label</a>" -v no="$NO_ORDER" \
  'index($0, a) { how = NR * 10000 + index($0, a) } index($0, b) { puts = NR * 10000 + index($0, b) } \
  END { print (!how || !puts) ? no : (puts > how ? "after" : "wrong") }' <<<"$head_panel")" after
# Nowhere else: the top bar's menu, the doors and the footer hold no copy of it,
# and no link of their own gains it. Each band is shown to be there first.
t_contains "$topbar" '<header class="topbar">'
t_contains "$doors" '<nav class="doors"'
t_contains "$footer_band" '<footer class="footer">'
t_lacks "$topbar" "$how_to_write_label"
t_lacks "$doors" "$how_to_write_label"
t_lacks "$footer_band" "$how_to_write_label"
t_lacks "$topbar" "$how_to_write"
# The one line of words under the title is given a width to wrap inside; beyond
# it the paragraph takes a line more and the fold moves. The bound is read from
# the stylesheet, not copied here.
t_eq "$(awk -v no="$NO_WIDTH" '/^\.floor-intro \{/ { on = 1 } on && match($0, /max-width: [0-9]+ch/) { print substr($0, RSTART + 11, RLENGTH - 11); found = 1 } on && /\}/ { on = 0 } END { if (!found) print no }' "$game")" 110ch

# The rows the stage names, in the order they are drawn.
stage_areas=$(awk '/^\.stage \{/,/^\}/' "$game" | grep -o '"[a-z ]*"' | tr '\n' ' ')

t_case "the host panel left core in 2.0: no section, link, chip or feed of it"
t_eq "$stage_areas" '"head head" "line monitor" "doors monitor" "board monitor" "console monitor" "epics epics" "break break" '
t_lacks "$page_text" 'host-view'
t_lacks "$page_text" 'href="#host"'
t_lacks "$page_text" 'floor-host'
t_lacks "$frag_text" 'top.json'
t_lacks "$frag_text" 'tehas-factory-host'

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
t_contains "$board_text" 'FACTORY PAUSED, FINISHING #'
t_contains "$board_text" ', still finishing #'
t_contains "$dashjs_text" 'if (s.paused) pill = ["pause", finishing.length ? "paused, finishing #"'
t_lacks "$dashjs_text" 'else if (s.paused) pill = ["pause", "paused"];'
t_contains "$manual_text" 'paused and, while a cycle is still running, that it is finishing that issue'
t_contains "$features_text" 'not proven that a cycle finishes while the factory is paused'

t_case "the title and the heading name tehas The Lean Software Factory, open source"
t_contains "$page_text" '<title>Tehas - The Lean Software Factory: open source, powered by Pi</title>'
t_contains "$page_text" '<meta property="og:title" content="Tehas - The Lean Software Factory: open source, powered by Pi">'
t_contains "$page_text" '<h1 id="floor-title">TEHAS: THE LEAN SOFTWARE FACTORY</h1>'
t_contains "$page_text" '<p class="floor-intro">The floor of an open source lean software factory'
t_contains "$page_text" '<a href="/lean-software-factory">HOW IT WORKS</a>'

t_done
