#!/usr/bin/env bash
# Tests for the manual: what it says about libexec/, the anchors it links
# to, and the environment its by-hand compose lines are run in. A statement
# about what a directory holds goes false when a program moves into or out of
# it, an anchor whose heading is renamed or moved lands a reader on the top of a
# page, and a compose line that never sourced the host's environment recreates a
# container from the defaults.
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
