#!/usr/bin/env bash
# Tests for the front page: where the live power meter stands and how much
# width it is given at each width of the window, what the header band is when
# the window is phone sized, and what answers a ticket row that is pressed.
# It reads the markup, the stylesheets and the scripts that draw into the
# markup as they are in the checkout: no browser, no server, no rendering.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

page=$T_ROOT/host/fun/index.html
game=$T_ROOT/host/fun/game-view.css
energy=$T_ROOT/host/fun/energy-view.css
script=$T_ROOT/host/fun/energy-view.js
board=$T_ROOT/host/fun/game-view.js
manual=$T_ROOT/host/docs/operating.md
features=$T_ROOT/host/docs/features.md
manual_text=$(cat "$manual")
features_text=$(cat "$features")
page_text=$(cat "$page")
game_text=$(cat "$game")
energy_text=$(cat "$energy")
script_text=$(cat "$script")
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

# The media query a line of a stylesheet sits in: the last @media above it, and
# of the lines that hold the pattern, the first one. NO_QUERY when the line is
# in the file with no media query around it, and NO_LINE when the line is not
# there at all, so that a changed value cannot read as the answer for a line
# outside every media query.
query_of() { # <holds this> <file>
  awk -v p="$1" '/@media/ { q = $0 } !seen && index($0, p) { seen = 1; found = q } END { print !seen ? "not in the file" : (found == "" ? "no media query" : found) }' "$2"
}
NO_QUERY="no media query" # a line of the file with no @media around it
NO_LINE="not in the file" # the pattern is not in the file at all

# How much room the meter needs, in the three parts the header comment of
# host/fun/energy-view.css names: the line itself, its padding, and the rule
# beside it. Topaz advances half an em: 8px a character at body size, 16px in
# display type. A token rate under a million a minute shows at most five
# figures, so the widest line is TOKENS with such a rate: the label of six
# characters, the gap between the label and the figures, five figures, the space
# before "/MIN" and the four characters of it.
line=$((6 * 8 + 8 + 5 * 16 + 8 + 4 * 8)) # 176: the line
padding=$((2 * 16)) # 32: its padding, 16px on each side
rule=4 # 4: the rule beside it
meter_widest=$((line + padding + rule)) # 212
# A width, or NO_WIDTH where the file does not hold the declaration, so that an
# absent declaration cannot read as a zero a meter would fit into.
NO_WIDTH="no width"
fits() { # <needs this many px> <has this, or no width>
  case "$2" in
    *[!0-9]*) echo "$NO_WIDTH" ;;
    *) [ "$1" -le "$2" ] && echo fits || echo "too wide" ;;
  esac
}

t_case "the live meter stands at the right end of the head panel"
t_contains "$head_panel" '<a class="floor-meter" id="floor-meter" href="#energy"'
t_eq "$(awk -v a='<p id="floor-summary">' -v b='<a class="floor-meter"' \
  'index($0, a) { last = NR } index($0, b) { meter = NR } END { print (last && meter > last) ? "after" : "wrong" }' "$page")" after

t_case "the meter is not one of the doors and nothing was added to the top bar"
t_lacks "$doors" 'meter'
t_lacks "$topbar" 'meter'
t_contains "$topbar" 'class="gate" id="gate"'   # the factory state gate still closes it
t_eq "$(awk '/class="door"/ { n++ } END { print n }' <<<"$doors")" 6

# The script reaches for an element by id. The dollar is written once here, so
# each case names only the id.
looks_up() { t_contains "$script_text" "\$(\"$1\")"; }

t_case "the script names the meter the page has, and no meter of its own"
looks_up floor-meter
looks_up energy
looks_up menu-energy
t_lacks "$script_text" 'door-meter'
t_lacks "$page_text" 'door-meter'

t_case "the panel splits for the meter only while the meter is there"
t_contains "$energy_text" '.floor-head:has(> .floor-meter:not([hidden])) {'
# The split names every row of the panel, so the meter stands over the title bar
# and the text under it rather than in a row of its own.
t_contains "$energy_text" 'grid-template-areas: "title meter" "intro meter" "foot meter";'
# The meter's own styles are in energy-view.css, not in the page's stylesheet.
t_eq "$(query_of '.floor-meter' "$game")" "$NO_LINE"

t_case "the meter costs 336px on a wide screen, where the monitor stands below it"
# The base column carries no media query: it is what a wide screen gets.
t_eq "$(query_of '--meter-column: 336px' "$energy")" "$NO_QUERY"
t_contains "$energy_text" 'grid-template-columns: minmax(0, 1fr) var(--meter-column);'

t_case "it costs 240px from 900 to 1279px, where the stage is one column"
t_eq "$(query_of '--meter-column: 240px' "$energy")" "@media (max-width: 1279px) {"

t_case "below 900px it lies across the foot of the panel at its full width"
t_eq "$(query_of 'grid-template-areas: "title" "intro" "foot" "meter";' "$energy")" "@media (max-width: 899px) {"
t_contains "$energy_text" '.floor-meter { border-top: 4px solid var(--outline); border-left: 0; }'

t_case "the meter fits the narrow column it is given, and the panel at 360px"
# The width the meter's column is given between 900 and 1280px, read out of the
# whole declaration rather than from an offset into it. A declaration that is
# not there answers NO_WIDTH, which fits nothing.
narrow=$(awk '/@media/ { q = $0 } !got && match($0, /--meter-column: [0-9]+px/) && index(q, "max-width: 1279px") { got = substr($0, RSTART, RLENGTH) } END { if (got == "") { print "no width"; exit } gsub(/[^0-9]/, "", got); print got }' "$energy")
t_eq "$narrow" 240
t_eq "$(fits "$meter_widest" "$narrow")" fits
# At 360px the stage has 16px of padding each side and the panel a 4px border.
# Below 900px the rule runs across the top of the meter rather than beside it, so
# counting it asks for 4px more than the meter needs there: the safe direction.
t_eq "$(query_of '.stage { padding: 0 16px; }' "$game")" "@media (max-width: 699px) {"
panel_360=$((360 - 2 * 16 - 2 * 4))
t_eq "$(fits "$meter_widest" "$panel_360")" fits

t_case "the meter does not reach into the sticky header or its height"
t_contains "$game_text" '--header: 80px;'
t_lacks "$energy_text" '.topbar'
t_lacks "$energy_text" '.door.meter'

t_case "the manual says the meter stands on the head panel, not among the doors"
t_contains "$manual_text" 'live meter stands at the right end of the panel'
t_lacks "$manual_text" 'meter stands among the doors'
t_contains "$features_text" 'live meter on the head panel'
t_lacks "$features_text" 'meter among the doors'

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

# The first rule that opens with this, at the start of its line or after the
# two spaces of a media block: the rule and its declarations, to the line that
# closes it. A grouped selector such as .station, .door, .monitor { is not it.
rule() { # <opening line starts with this>
  awk -v hold="$1" '!on && (index($0, hold) == 1 || index($0, hold) == 3) { on = 1 }
    on { print } on && index($0, "}") { exit }' "$game"
}

# Every rule that opens with this selector and holds this declaration, printed
# whole. The stylesheet names .monitor more than once; only one of those rules
# is the wide sticky one.
rules_holding() { # <opens with this> <declaration line holds this>
  awk -v sel="$1" -v want="$2" '
    index($0, sel) == 1 || index($0, sel) == 3 { on = 1; body = "" }
    on { body = body $0 "\n" }
    on && index($0, "}") {
      on = 0
      if (index(body, want)) printf "%s", body
    }
  ' "$game"
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

# The work order is a panel in the column below 1280px, so nothing in the desk
# travels under the header while the page scrolls. The wide monitor beside the
# stations is the one that sticks, and it is outside every media block.
t_case "the phone layout pins nothing, and the wide monitor still sticks"
t_lacks "$phone" 'position: sticky'
t_contains "$(rule '.monitor { position: static')" 'position: static;'
t_eq "$(query_of '.monitor {' "$game")" "$NO_QUERY"
wide=$(rules_holding '.monitor {' 'position: sticky;')
t_contains "$wide" 'position: sticky;'
t_contains "$wide" 'top: calc(var(--header) + 16px);'

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

# The manual and the Protolab load game-view.css and run no script, so their
# clearance is the plain literal of the band. Their menu holds four links and
# keeps the one row that scrolls, so raising it would put the literal above the
# band those pages really render.
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

# Below 1280px the work order is a panel in the column: it can be off screen
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
t_contains "$game_text" '.row.is-current { background: var(--brass-light); }'
t_contains "$game_text" '.ticket:hover, .ticket.is-current { background: var(--brass-light); }'

t_case "the answer stays where it was pressed: selecting moves nothing on the page"
t_lacks "$select_fn" 'scrollIntoView'
t_lacks "$select_fn" 'scrollTo'
t_lacks "$select_fn" 'window.scroll'

t_done
