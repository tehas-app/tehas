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

# The media query a line of a stylesheet sits in: the last @media above it.
# "none" when the line is in the file with no media query around it, and
# "not in the file" when the line is not there at all, so that a changed value
# cannot read as the answer for a line outside every media query.
query_of() { # <holds this> <file>
  awk -v p="$1" '!seen && index($0, p) { seen = 1; found = q } /@media/ { q = $0 } END { print !seen ? "not in the file" : (found == "" ? "none" : found) }' "$2"
}

# How much room the meter needs, the same count as the header comment of
# host/fun/energy-view.css. Topaz advances half an em: 8px a character at body
# size, 16px in display type. A token rate under a million a minute shows at
# most five figures, so the widest line is TOKENS with such a rate: the label of
# six characters, the gap between the label and the figures, five figures, the
# space before "/MIN" and the four characters of it, the band's 16px of padding
# on each side, and the 4px rule beside it. Below 900px the rule moves to the
# top, so counting it makes the figure 4px more than the meter needs there.
meter_widest=$((6 * 8 + 8 + 5 * 16 + 8 + 4 * 8 + 2 * 16 + 4)) # 212
fits() { [ "$1" -le "$2" ] && echo fits || echo "too wide"; }

t_case "the live meter stands at the right end of the head panel"
t_contains "$head_panel" '<a class="floor-meter" id="floor-meter" href="#energy"'
t_eq "$(awk -v a='<p id="floor-summary">' -v b='<a class="floor-meter"' \
  'index($0, a) { last = NR } index($0, b) { meter = NR } END { print (last && meter > last) ? "after" : "wrong" }' "$page")" after

t_case "the meter is not one of the doors and nothing was added to the top bar"
t_lacks "$doors" 'meter'
t_lacks "$topbar" 'meter'
t_contains "$topbar" 'class="gate" id="gate"'   # the factory state gate still closes it
t_eq "$(awk '/class="door"/ { n++ } END { print n }' <<<"$doors")" 6

t_case "the script names the meter the page has, and no meter of its own"
t_contains "$script_text" "\$(\"floor-meter\")"
t_contains "$script_text" "\$(\"energy\")"
t_contains "$script_text" "\$(\"menu-energy\")"
t_lacks "$script_text" 'door-meter'
t_lacks "$page_text" 'door-meter'

t_case "the panel splits for the meter only while the meter is there"
t_contains "$energy_text" '.floor-head:has(> .floor-meter:not([hidden])) {'
# The split names every row of the panel, so the meter stands over the title bar
# and the text under it rather than in a row of its own.
t_contains "$energy_text" 'grid-template-areas: "title meter" "intro meter" "foot meter";'
t_lacks "$game_text" 'floor-meter'

t_case "the meter costs 336px on a wide screen, where the monitor stands below it"
# The base column carries no media query: it is what a wide screen gets.
t_eq "$(query_of '--meter-column: 336px' "$energy")" "none"
t_contains "$energy_text" 'grid-template-columns: minmax(0, 1fr) var(--meter-column);'

t_case "it costs 240px from 900 to 1279px, where the stage is one column"
t_eq "$(query_of '--meter-column: 240px' "$energy")" "@media (max-width: 1279px) {"

t_case "below 900px it lies across the foot of the panel at its full width"
t_eq "$(query_of 'grid-template-areas: "title" "intro" "foot" "meter";' "$energy")" "@media (max-width: 899px) {"
t_contains "$energy_text" '.floor-meter { border-top: 4px solid var(--outline); border-left: 0; }'

t_case "the meter fits the narrow column it is given, and the panel at 360px"
# The number the meter's column is given between 900 and 1280px, read out of
# the declaration itself. A declaration that is not there reads as 0, which
# fits nothing.
narrow=$(awk '/@media/ { q = $0 } !got && match($0, /--meter-column: [0-9]+px/) && q ~ /max-width: 1279px/ { got = substr($0, RSTART + 16, RLENGTH - 18) } END { print got + 0 }' "$energy")
t_eq "$(fits "$meter_widest" "$narrow")" fits
# At 360px the stage has 16px of padding each side and the panel a 4px border.
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
