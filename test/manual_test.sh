#!/usr/bin/env bash
# Tests for the manual: what it says about libexec/, the anchors it links to,
# the pages the commands are split over, the commands its lists name, and the
# environment its by-hand compose lines are run in. A statement about what a
# directory holds goes false when a program moves into or out of it, an anchor
# whose heading is renamed or moved lands a reader on the top of a page, a
# command the front routes and no page names is a command nobody can find, and
# a compose line that never sourced the host's environment recreates a container
# from the defaults.
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

# ---------------------------------------------------------------------------
# The commands the manual names. The front of the two commands routes one word
# after `tehas` or `tehasbot` (console/src/front.rs); a command it routes and no
# page of the commands names is a command no person can find, which is what the
# summary blocks of tools.md are for.

# t_front_verbs <tehas|tehasbot> <file>: the words the front routes for that
# command, one per line, read from the source of the console rather than from a
# list written here: a verb added to the front is a verb this case asks for. The
# `tehas` verbs are the entries of VERBS plus the two commands that are a script
# of libexec. The `tehasbot` verbs are the words of the one match of tehasbot,
# from its opening to `Some(c)`, the arm that answers a word none of the others
# is. Words of a test are not read: the source holds the same lists in its own
# cases. The list is written to a file and not returned down a pipe, as t_anchors
# says, so a grep that closes early is no failure of the case.
t_front_verbs() { # <tehas|tehasbot> <file>
  local which=$1 file=$2
  case $which in
  tehas)
    sed -n '/^const VERBS: /,/^];/p' console/src/front.rs | grep -o '"[a-z-]*"' |
      sed 's/"//g' >"$file" || true
    printf '%s\n' setup update >>"$file"
    ;;
  tehasbot)
    # The words of the one match of tehasbot, which are its commands. Read up to
    # the arm that answers a word none of them is, and no further: the words of
    # that arm's fall name a script of libexec, and tehasbot has none.
    sed -n '/"health" | "status"/,/Some(c) => script("tehasbot"/p' console/src/front.rs \
      >"$T/front-words" || true
    # The route's own words, and not the words of a pattern that only asks what
    # the word after the command is: the guard of the `mcp` arm reads the second
    # word, so `serve` is no command of its own, and `tehasbot` is the name of
    # the program in the name of the arm this stops at.
    grep -o '"[a-z-]*"' "$T/front-words" | sed 's/"//g' | sort -u |
      grep -vx serve | grep -vx tehasbot >"$file" || true
    rm -f "$T/front-words"
    ;;
  *) t_bad "no verbs read for $which" ;;
  esac
}

# t_lists_of <tehas|tehasbot> <file>: the commands of that program named at the
# head of a line of the fenced command blocks of the tools pages. The block of a
# summary is a fence whose lines open with the program's name, a space and a word
# that is a command's name, so a prose line that names a command in a sentence is
# not one read here, nor an example's output such as `tehas v0.8.0`, nor a
# command of a fence of the other program.
t_lists_of() { # <tehas|tehasbot> <file>
  local which=$1 file=$2
  awk -v prog="$which" '
    /^```/ { infence = !infence; next }
    infence && index($0, prog " ") == 1 { if (substr($2, 1, 1) ~ /[a-z]/) print $2 }
  ' "${TOOLS_PAGES[@]}" | sort -u >"$file"
}

# The two readers before they judge anything: a rule that found no verb, or no
# command of the lists, would answer every command below with nothing and pass.
t_case "the front's verbs and the lists of the pages are both read"
t_front_verbs tehas "$T/verbs-tehas"
t_front_verbs tehasbot "$T/verbs-tehasbot"
t_eq "$(sort -u "$T/verbs-tehas" | grep -c . | tr -d ' ')" 29
t_eq "$(sort -u "$T/verbs-tehasbot" | grep -c . | tr -d ' ')" 15
t_lists_of tehas "$T/list-tehas"
t_lists_of tehasbot "$T/list-tehasbot"
[ "$(grep -c . "$T/list-tehas" | tr -d ' ')" -ge 25 ] ||
  t_bad "the command blocks name 25 tehas commands or fewer"
[ "$(grep -c . "$T/list-tehasbot" | tr -d ' ')" -ge 13 ] ||
  t_bad "the command blocks name 13 tehasbot commands or fewer"

# Every command the front routes is named in a command block of the pages, which
# is where a person looks for it. A word the blocks name is matched as a whole
# argument of the line, so `preview` of the one block is the command of a line
# of `tehasbot preview --sweep`, and a line of `tehas state pack` is no line of
# `tehas state copy`.
t_case "every command the front routes is named in the command blocks of the pages"
for which in tehas tehasbot; do
  while IFS= read -r word; do
    [ -n "$word" ] || continue
    case $word in
    help) continue ;;
    esac
    found=0
    for page in "${TOOLS_PAGES[@]}"; do
      awk -v prog="$which" -v cmd="$word" '
        /^```/ { infence = !infence; next }
        infence && index($0, prog " ") == 1 && substr($2, 1, 1) ~ /[a-z]/ {
          for (i = 2; i <= NF; i++) if ($i == cmd) { print "yes"; exit }
        }
      ' "$page" | grep -q yes && { found=1; break; }
    done
    [ "$found" = 1 ] || echo "$which $word: no command block of the tools pages names it"
  done < <(sort -u "$T/verbs-$which")
done >"$T/tmp-verbs" 2>&1
t_eq "$(wc -l < "$T/tmp-verbs" | tr -d ' ')" 0

# The commands the front does not route, told from the ones it does: a case that
# read the verbs of the front as nothing, or read them from a list written here,
# would keep its silence whatever the source said. Each half of the pair is one
# word the blocks name and the front routes, and one of each program that no
# page names and no route answers.
t_case "the front's verbs are its own, and not the commands of the pages"
printf '%s\n' bogus | grep -qxF -f "$T/verbs-tehas" &&
  t_bad "the verbs read hold bogus, a command no route answers"
grep -qxF -f "$T/list-tehas" <(printf 'work\n') ||
  t_bad "the command blocks read hold no work, which every list of tehas gives"
printf '%s\n' bogus | grep -qxF -f "$T/verbs-tehasbot" &&
  t_bad "the verbs read hold bogus, a command no route answers"
grep -qxF -f "$T/list-tehasbot" <(printf 'clean\n') ||
  t_bad "the command blocks read hold no clean, which the block of tehasbot gives"

# A command named in the source as one that does not exist: no route answers it,
# and no page gives it a section. `tehas tidy` is the one this page had, which
# is `console tidy run`, and `tehasbot work` the other a review of the manual
# found.
t_case "no page names a command that is not a verb of either program"
for word in 'tehas tidy' 'tehasbot work'; do
  hits=$(grep -rn "$word" "${TOOLS_PAGES[@]}" "$docs/operating.md" "$docs/features.md" \
    "$docs/tools-health.md" 2>/dev/null || true)
  [ -z "$hits" ] || t_bad "names a command that is not one: $hits"
done

# ---------------------------------------------------------------------------
# The compose lines of the manual, and the environment they are run in. The two
# files of host/edge read TEHAS_DESIGN, TEHAS_INDEXING, TEHAS_ANALYTICS_* and
# TEHAS_HOST_ADDR from the shell that starts them, so a line that runs compose
# straight recreates a container from the defaults: a host of TEHAS_DESIGN=pixel
# is drawn in beesknees26, and the ports bind to 127.0.0.1. `edge/` stands for
# `~/tehas/edge/`, the install layout the pages write, so the rule reads every
# page of the manual and nothing else. `up` is the only command asked about: it
# is the one whose result is a running container, and so the one the environment
# of the shell reaches. `down` takes containers down and reads nothing of the
# settings.
compose_line() {
  grep -rn 'compose -f [^ ]*edge/[^ ]* up' "$@" || true
}

# A line of the manual may write `. ~/tehas/env.sh` itself, or the page may
# source it on the line before, as a fenced block that runs several commands
# does. Nothing else counts: the env of the shell the reader is in is not the
# env of the container compose makes. The source command is asked for by its
# own shape, dot, space, the path: a line that only names the file in prose is
# not a shell that read it.
env_sourced() { # <file> <line number of the compose line>
  local file=$1 line=$2 before
  before=$(sed -n "$((line - 1))p" "$file")
  [ "${before//. ~/tehas\/env.sh/}" != "$before" ]
}

# Read <page> from the front, one line each, and print the compose commands as
# `number<TAB>text`. `grep -r` of a single file leaves the `file:line:` prefix
# off, which is why the numbers are taken from the page and not from `grep`: a
# scratch page of one has no line number to read off its matches. `up` is asked
# for as a word, as `compose_line`'s pattern does: a path that happens to hold
# `up` names no command.
compose_numbered() { # <page>
  local page=$1 n=0 text rest
  while IFS= read -r text; do
    n=$((n + 1))
    case $text in
    *compose\ -f\ *edge/*up*)
      rest=${text##*up}
      case $rest in
      '') printf '%s\t%s\n' "$n" "$text" ;;
      [\ \']*) printf '%s\t%s\n' "$n" "$text" ;;
      esac
      ;;
    esac
  done <"$page"
}

# compose_found <page>...: how many commands the finder reads in these pages,
# counted from the page rather than from grep's own report.
compose_found() { # <page>...
  local page n
  n=0
  for page in "$@"; do n=$((n + $(compose_numbered "$page" | grep -c . || true))); done
  printf '%s\n' "$n"
}

# The judge, in one function, so the manual and the scratch pages of the cases
# below are read by the same code. It prints one `OK <page>:<line>` or
# `BAD <page>:<line>` per command it examined, and nothing for a page that
# holds no compose command. The verdict is a word rather than the presence of
# output: a case that asked only for "no output" would pass whether the judge
# worked, said nothing, or was taken away.
compose_verdicts() { # <page>...
  local page n text
  for page in "$@"; do
    while IFS=$'\t' read -r n text; do
      case $text in
      *.\ ~/tehas/env.sh*) printf 'OK %s:%s\n' "$page" "$n" ;;
      *)
        if env_sourced "$page" "$n"; then
          printf 'OK %s:%s\n' "$page" "$n"
        else
          printf 'BAD %s:%s\n' "$page" "$n"
        fi
        ;;
      esac
    done < <(compose_numbered "$page")
  done
}

# t_verdict <want> <count> <total> <page>...: of <total> commands the judge
# examined in these pages, <count> got <want> and the rest the other verdict.
# Both numbers are asked for: the total says the rule had something to judge, the
# count that it judged it the right way. A page whose commands are fewer or more
# than <total> fails, so the judge cannot be widened onto lines it should not
# read or shrunk off the ones it should.
t_verdict() { # <OK|BAD> <count> <total> <page>...
  local want=$1 count=$2 total=$3
  shift 3
  local seen got
  seen=$(compose_verdicts "$@")
  got=$(printf '%s\n' "$seen" | grep -c "^$want " || true)
  # The verdicts are asked for by name, not by their count alone: the check
  # passes only when the judge said what is expected of it, over the same pages.
  # A judge silenced, mislabelled, or pointed at another line says otherwise.
  if [ "$seen" = "$(t_verdict_expected "$want" "$total" "$@")" ] && [ "$got" = "$count" ]; then
    return 0
  fi
  t_bad "$want of $count in $total commands: got $got. [$(printf '%s\n' "$seen" | tr '\n' ' ')]"
}

# t_verdict_expected <OK|BAD> <total> <page>...: the verdicts the judge must
# print for these pages. It reads the pages again through `env_sourced` and the
# source-on-the-line test rather than echoing the judge, so a judge silenced or
# turned the wrong way prints something other than this. Nothing when the count
# of commands is not <total>, which is what makes the total a real ask.
t_verdict_expected() { # <OK|BAD> <total> <page>...
  local want=$1 total=$2
  shift 2
  local page n text out=() got
  for page in "$@"; do
    while IFS=$'\t' read -r n text; do
      case $text in
      *.\ ~/tehas/env.sh*) out+=("OK $page:$n") ;;
      *)
        if env_sourced "$page" "$n"; then out+=("OK $page:$n"); else out+=("BAD $page:$n"); fi
        ;;
      esac
    done < <(compose_numbered "$page")
  done
  got=${#out[@]}
  [ "$got" = "$total" ] || return 0
  [ "${want}" = OK ] || [ "${want}" = BAD ] || return 0
  printf '%s\n' "${out[@]}"
}

# t_reads <count> <page>...: the judge examined <count> commands of these pages,
# whatever it made of them. The count is what says the run had something to judge:
# a page read as nothing is told from a page read and found sound.
t_reads() { # <count> <page>...
  local count=$1
  shift
  local seen
  seen=$(compose_verdicts "$@")
  if [ "$seen" = "$(t_reads_expected "$count" "$@")" ]; then
    return 0
  fi
  t_bad "expected to read $count commands, got $(printf '%s\n' "$seen" | grep -c . || true). [$(printf '%s\n' "$seen" | tr '\n' ' ')]"
}

# t_reads_expected <count> <page>...: the verdicts a page that holds <count>
# commands must print, whatever they are. Nothing when the count is not the
# count of commands the page holds, so a judge that read none cannot agree.
t_reads_expected() { # <count> <page>...
  local count=$1
  shift
  local page n text out=()
  for page in "$@"; do
    while IFS=$'\t' read -r n text; do
      case $text in
      *.\ ~/tehas/env.sh*) out+=("OK $page:$n") ;;
      *)
        if env_sourced "$page" "$n"; then out+=("OK $page:$n"); else out+=("BAD $page:$n"); fi
        ;;
      esac
    done < <(compose_numbered "$page")
  done
  [ "${#out[@]}" = "$count" ] || return 0
  printf '%s\n' "${out[@]}"
}

# t_row <command> <page>: the row of the page that runs <command>, required to
# carry the source on that same line. The answer names the command and the text of
# the row it was found on, so a row whose prefix went missing again is the one the
# list of answers below says is missing.
t_row() { # <command> <page>
  local command=$1 page=$2 line
  line=$(compose_numbered "$page" |
    while IFS=$'\t' read -r number text; do
      # The command is asked for as the last thing in the cell, the backtick
      # that closes it read by its code point: the row of `... up -d` is not
      # the row of `... up -d --force-recreate`, whose text holds the shorter
      # command as well, so a plain substring match would answer for both.
      backtick=$(printf '\140')
      case $text in *"$command"$backtick*) printf '%s\t%s\n' "$number" "$text"; break ;; esac
    done)
  if [ -n "$line" ] && printf '%s\n' "${line#*$'\t'}" |
    grep -qF ". ~/tehas/env.sh && $command"; then
    return 0
  fi
  t_bad "the row running [$command] does not source env.sh on its own line: [$line]"
}

# Each fixture below writes its own page, named before the first of them runs so
# no case writes to a name the run never meant to open. They are separate files
# because each case asks about one page on its own: a single page reused by every
# case would let the verdicts of one case stand for another's.
fixture_bare=$T/manual-compose-bare.md
fixture_prefix=$T/manual-compose-prefix.md
fixture_above=$T/manual-compose-above.md
fixture_prose=$T/manual-compose-prose.md
fixture_down=$T/manual-compose-down.md
fixture_status=$T/manual-compose-status.md
fixture_mixed=$T/manual-compose-mixed.md

# The manual as it stands, judged by the rule the fixtures below are shown the
# same way. Two asks, one for each half of what "the manual is clean" means: that
# the rule found the ten commands the finder finds, and that it found none of them
# without the host's environment.
t_case "the manual runs compose over a file of host/edge, so the judge has work"
t_verdict OK 10 10 "${pages[@]}"

t_case "every compose line of the manual reads env.sh first, on its own line or the one above"
t_verdict BAD 0 10 "${pages[@]}"

# The negative fixtures: pages made for the rule, judged by the same function that
# judges the manual. A rule that matched nothing, or that matched and kept its
# finding to itself, would keep the four lines that were written without the
# prefix, so the rule is shown giving BAD to text it is meant to give BAD to.
t_case "the judge gives BAD to a compose line that never saw env.sh"
printf '%s\n' 'Start the router:' '' '```' \
  'docker compose -f ~/tehas/edge/router.compose.yaml up -d' '```' >"$fixture_bare"
t_verdict BAD 1 1 "$fixture_bare"

t_case "the judge gives OK to the same line when it carries the prefix"
printf '%s\n' '```' \
  '. ~/tehas/env.sh && docker compose -f ~/tehas/edge/router.compose.yaml up -d' '```' >"$fixture_prefix"
t_verdict OK 1 1 "$fixture_prefix"

t_case "the judge gives OK when the page sources env.sh on the line above"
printf '%s\n' '```' '. ~/tehas/env.sh' \
  'docker compose -f ~/tehas/edge/docs.compose.yaml up -d --force-recreate' '```' >"$fixture_above"
t_verdict OK 1 1 "$fixture_above"

# A line above that only names the file is not a shell that read it: the
# container would still be made from the defaults.
t_case "the judge gives BAD when the line above only names env.sh in prose"
printf '%s\n' '```' '# read ~/tehas/env.sh before you start' \
  'docker compose -f ~/tehas/edge/router.compose.yaml up -d' '```' >"$fixture_prose"
t_verdict BAD 1 1 "$fixture_prose"

t_case "a line that takes the container down is not one the judge asks about"
printf '%s\n' '```' 'docker compose -f ~/tehas/edge/router.compose.yaml down' '```' >"$fixture_down"
t_reads 0 "$fixture_down"
t_eq "$(compose_line "$fixture_down" | wc -l)" 0

# The word bound on `up`: a file whose name happens to hold `up` is not the
# command, and a page of it is nothing to judge.
t_case "a path that holds up but names no command is not one the judge reads"
printf '%s\n' '```' 'docker compose -f ~/tehas/edge/backup.compose.yaml status' '```' >"$fixture_status"
t_reads 0 "$fixture_status"
t_eq "$(compose_line "$fixture_status" | wc -l)" 0

# One page holding both a good and a bad command: the good one does not stand
# for the bad one, and the judge is not silenced by the first.
t_case "the judge reads every command of a page, and reports each on its own"
# The judge is not silenced once a page has read the file: the command that
# follows a good one is judged on its own, and a page whose first line is sound
# does not carry a later one that is not.
printf '%s\n' '```' '. ~/tehas/env.sh && docker compose -f ~/tehas/edge/docs.compose.yaml up -d' \
  'docker compose -f ~/tehas/edge/router.compose.yaml up -d' \
  'echo the settings of the host' 'docker compose -f ~/tehas/edge/router.compose.yaml up -d --force-recreate' '```' >"$fixture_mixed"
t_reads 3 "$fixture_mixed"
t_verdict OK 2 3 "$fixture_mixed"
t_verdict BAD 1 3 "$fixture_mixed"

# The four by-hand lines of host.md the issue named, each found by the command
# it runs rather than by a number that moves when the page grows: the row of
# `docs`, the row of `edge`, and the two rows of the Services table. The case
# asks that each of the four carries the prefix on its own line, and that a
# prefix sitting on the line above is not taken for one, which is what tells
# this rule apart from the line-above reading a fenced block is allowed.
t_case "the four by-hand lines of host.md each carry the prefix on their own line"
t_reads 4 "$docs/host.md"
t_verdict OK 4 4 "$docs/host.md"
# Each of the four rows, by the command it runs, must carry the source on its own
# line. The command is the whole of the cell: the row of `... up -d` is not the
# row of `... up -d --force-recreate`, whose text holds the shorter command too.
t_row 'docker compose -f ~/tehas/edge/docs.compose.yaml up -d --force-recreate' "$docs/host.md"
t_row 'docker compose -f ~/tehas/edge/router.compose.yaml up -d --force-recreate' "$docs/host.md"
t_row 'docker compose -f ~/tehas/edge/docs.compose.yaml up -d' "$docs/host.md"
t_row 'docker compose -f ~/tehas/edge/router.compose.yaml up -d' "$docs/host.md"

# The count of compose commands the manual writes, asked for twice, by two reads
# of the pages that share nothing but the pattern: `grep` for one and a line by
# line pass for the other. The judge is given the totals of its own choosing, so
# were it shrunk off the lines it should read or widened onto lines it should not,
# its own totals would go quietly wrong and this case would still say how many
# commands the manual really writes. Ten, at the time of writing: four in the two
# tables of host.md, two each in api.md and tools.md, one in edge.md, and one more
# in troubleshooting.md besides the line that takes the containers down.
t_case "the finder and the judge read the same ten commands of the manual"
t_eq "$(compose_line "${pages[@]}" | wc -l)" 10
t_eq "$(compose_found "${pages[@]}")" 10

t_done
