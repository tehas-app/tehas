#!/usr/bin/env bash
# Tests for the front page: where the live power meter stands and how much
# width it is given at each width of the window. It reads the markup, the
# stylesheets and the script that draws into the markup as they are in the
# checkout: no browser, no server, no rendering.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh"

page=$T_ROOT/host/fun/index.html
game=$T_ROOT/host/fun/game-view.css
energy=$T_ROOT/host/fun/energy-view.css
script=$T_ROOT/host/fun/energy-view.js
manual=$T_ROOT/host/docs/operating.md
features=$T_ROOT/host/docs/features.md
manual_text=$(cat "$manual")
features_text=$(cat "$features")
page_text=$(cat "$page")
game_text=$(cat "$game")
energy_text=$(cat "$energy")
script_text=$(cat "$script")

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

t_done
