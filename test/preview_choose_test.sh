#!/usr/bin/env bash
# Tests for console preview choose: the choice of which open pull requests get
# a preview (console/src/preview.rs). These are the rules the header of
# libexec/preview states, run against the console binary itself; the shell
# tests of preview_authors_test.sh hold the same list end to end.
#
# The jq calls here name their file, as the preview library's do: the jq of
# some hosts reads a file argument as no input at all when standard input is
# still the terminal, and a filter run on no input answers null and exit 0.
# shellcheck disable=SC2016  # the jq programs here are jq's
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck disable=SC2154  # t_console_why is set by t_console (test/lib.sh)
t_console || t_bad "no console: $t_console_why"

# The commit of pull request <n> in the rows these cases make: 39 zeros and
# the number's last digit, as the preview fixtures build one.
sha_of() { printf '0%.0s' $(seq 39); printf '%s' "$(( $1 % 10 ))"; }

# pull <number> [head repository] [association]: one row of the list as GitHub
# answers it. Of this repository, opened by a member, when nothing else is
# said. The body is the description GitHub holds, which is where a notice of
# the run goes, so a row of these is a pull request the run could tell about.
pull() {
  jq -cn --argjson n "$1" --arg repo "${2-acme/widget}" --arg who "${3-MEMBER}" '
    {number: $n, title: "Change \($n)",
     html_url: "https://github.com/acme/widget/pull/\($n)", state: "open",
     updated_at: "2026-10-11T09:00:00Z",
     head: {sha: (("0" * 39) + (($n % 10) | tostring)),
            repo: (if $repo == "" then null else {full_name: $repo} end)},
     body: "What the change does."}
    + (if $who == "" then {} else {author_association: $who} end)'
}

# page_of <rows...>: those rows as one page, which is the shape GitHub answers
# and the shape the run hands the console.
page_of() {
  printf '%s\n' "$@" | jq -cn '[inputs]'
}

# choose <max> <rows...>: the console's answer for these pull requests. The
# rows are the open pull requests of one page, which is what GitHub answers
# and what the run keeps for the console, so they go in as an array.
choose() {
  local max=$1
  shift
  printf '%s\n' "$@" | jq -cn '[inputs]' |
    "$T_CONSOLE" preview choose acme/widget "$max"
}

t_case "one of ours is built, with its commit and the time GitHub gave"
t_eq "$(choose 5 "$(pull 12)")" "build 12 $(sha_of 12) 2026-10-11T09:00:00Z"
t_eq "$t_rc" 0

# Whoever opened it: a fork's head is not in the product repository, so its
# code is not the repository's.
t_case "a pull request from a fork is never chosen, whoever opened it"
for who in OWNER MEMBER COLLABORATOR NONE; do
  t_eq "$(choose 5 "$(pull 31 stranger/widget "$who")")" "skip elsewhere 31" "opened by $who"
done

t_case "a head whose repository is gone is a fork, not an unclear answer"
t_eq "$(choose 5 "$(pull 32 "")")" "skip elsewhere 32"

t_case "an outsider between two branches of the repository is never chosen"
for who in NONE CONTRIBUTOR FIRST_TIME_CONTRIBUTOR FIRST_TIMER MANNEQUIN member Owner; do
  t_eq "$(choose 5 "$(pull 33 acme/widget "$who")")" "skip outside 33" "opened by $who"
done

t_case "an owner, a member and a collaborator are chosen"
for who in OWNER MEMBER COLLABORATOR; do
  t_eq "$(choose 5 "$(pull 34 acme/widget "$who")")" "build 34 $(sha_of 34) 2026-10-11T09:00:00Z" "$who"
done

t_case "the newest max are built and the rest say over-limit"
t_eq "$(choose 2 "$(pull 26)" "$(pull 25)" "$(pull 24)" "$(pull 21)" | awk '{print $1, $2}')" \
  "$(printf 'build 26\nbuild 25\nover-limit 24\nover-limit 21')"
# With room for four of them the same list is read as four builds: what makes
# a row over-limit is the limit of the run, not the pull request.
t_eq "$(choose 4 "$(pull 26)" "$(pull 25)" "$(pull 24)" "$(pull 21)" | awk '{print $1}' | sort -u)" "build"
# The over-limit rows carry the commit too: the run stops that container and
# marks the state with it.
t_contains "$(choose 2 "$(pull 26)" "$(pull 25)" "$(pull 24)")" "over-limit 24 $(sha_of 24) "

t_case "a fork or an outsider takes no place from the others"
t_eq "$(choose 2 "$(pull 29 acme/widget NONE)" "$(pull 28 acme/widget CONTRIBUTOR)" \
  "$(pull 26 stranger/widget NONE)" "$(pull 22)" "$(pull 21)" | awk '$1 == "build" {print $2}' | tr '\n' ' ')" \
  "22 21 "
t_eq "$(choose 2 "$(pull 29 acme/widget NONE)" "$(pull 26 stranger/widget NONE)" "$(pull 22)")" \
  "$(printf 'build 22 %s 2026-10-11T09:00:00Z\nskip outside 29\nskip elsewhere 26' "$(sha_of 22)")"


t_case "a list that does not say who opened one of ours chooses nothing"
# The row of #16 has its head here and an association that says nothing: the
# field gone, null, or not text. None of them is read as an outsider's, which
# would take the preview from #12, the one that serves. The filter arrives as
# data and the row as a named file: see the note at the top of this file about
# the jq of some hosts, which reads a file argument as no input at all.
scratch_dir=$(mktemp -d)
printf '%s\n' "$(pull 16)" >"$scratch_dir/p16.json"
printf '%s\n' 'del(.author_association)' >"$scratch_dir/f1.txt"
printf '%s\n' '.author_association = null' >"$scratch_dir/f2.txt"
printf '%s\n' '.author_association = ["OWNER"]' >"$scratch_dir/f3.txt"
printf '%s\n' '.author_association = true' >"$scratch_dir/f4.txt"
for n in 1 2 3 4; do
  jq -cn --slurpfile row "$scratch_dir/p16.json" --rawfile f "$scratch_dir/f$n.txt" \
    '$row[0] | if $f == "del(.author_association)\n" then del(.author_association)
      elif $f == ".author_association = null\n" then .author_association = null
      elif $f == ".author_association = [\"OWNER\"]\n" then .author_association = ["OWNER"]
      else .author_association = true end' </dev/null >"$scratch_dir/q.json"
  t_eq "$(choose 5 "$(pull 12)" "$(cat "$scratch_dir/q.json")")" \
    "the list of open pull requests does not say who opened each" "filter $n"
  t_eq "$t_rc" 0
done

t_case "a fork that does not say who opened it is still counted as a fork"
# The association is only asked for of a head that is here, so a fork with no
# association is a fork and stops nothing.
printf '%s\n' "$(pull 26 stranger/widget)" >"$scratch_dir/p26.json"
jq -cn --slurpfile row "$scratch_dir/p26.json" '$row[0] | del(.author_association)' \
  </dev/null >"$scratch_dir/f.json"
t_eq "$(choose 5 "$(pull 12)" "$(cat "$scratch_dir/f.json")")" \
  "$(printf 'build 12 %s 2026-10-11T09:00:00Z\nskip elsewhere 26' "$(sha_of 12)")"

t_case "a page that is not a page chooses nothing"
for given in 'not json at all' '{"number":' '['; do
  t_eq "$(printf '%s' "$given" | "$T_CONSOLE" preview choose acme/widget 5)" "nothing read" "$given"
  t_eq "$t_rc" 0
done

t_case "an empty list chooses nothing and says nothing"
# A page with no pull request in it is a list that was read, which chooses
# nothing and says nothing. Bytes that are no page at all are a list that
# cannot be read, which says so.
t_eq "$(choose 5)" ""
t_eq "$t_rc" 0
t_eq "$(printf '[]\n' | "$T_CONSOLE" preview choose acme/widget 5)" ""
t_eq "$(printf '[]\n[]\n' | "$T_CONSOLE" preview choose acme/widget 5)" ""
t_eq "$(printf '' | "$T_CONSOLE" preview choose acme/widget 5)" "nothing read"

t_case "a head that is not a commit id builds nothing and is not counted"
for sha in 'main; id' '' 'abc' "$(printf 'z%.0s' $(seq 40))" "$(printf 'a%.0s' $(seq 39))"; do
  given=$(jq -cn --arg s "$sha" '.head.sha = $s' <<<"$(pull 15)")
  t_eq "$(choose 5 "$given")" "" "$sha"
done

t_case "a number that is not one builds nothing"
for number in 0 -3 1000000000 '"12"' null; do
  given=$(jq -cn --argjson n "$number" '.number = $n' <<<"$(pull 12)")
  t_eq "$(choose 5 "$given")" "" "$number"
done

t_case "the same pull request on two pages counts once"
twice=$(pull 12)
t_eq "$(choose 2 "$twice" "$twice" "$(pull 11)" | awk '$1 == "build" {print $2}' | tr '\n' ' ')" "12 11 "

t_case "two pages are read as one list"
given=$(printf '%s\n%s\n' "$(page_of "$(pull 12)")" "$(page_of "$(pull 11)")")
t_eq "$(printf '%s' "$given" | "$T_CONSOLE" preview choose acme/widget 5 | tr '\n' ' ')" \
  "build 12 $(sha_of 12) 2026-10-11T09:00:00Z build 11 $(sha_of 11) 2026-10-11T09:00:00Z "

t_case "the repository is compared without regard to capitals"
t_eq "$(choose 5 "$(pull 12 ACME/Widget)")" "build 12 $(sha_of 12) 2026-10-11T09:00:00Z"

t_case "max 0 leaves every one of ours over-limit"
t_eq "$(choose 0 "$(pull 12)" | awk '{print $1}')" "over-limit"

t_case "a missing argument or a max that is not a number is usage"
t_eq "$(printf '' | "$T_CONSOLE" preview choose 2>&1; echo "rc=$?")" "$(
  printf 'console preview choose: usage: <repo> [max] <pull requests\nrc=2')"
t_eq "$(printf '' | "$T_CONSOLE" preview choose acme/widget many 2>&1; echo "rc=$?")" \
  "$(printf 'console preview choose: <max> is not a whole number: many\nrc=2')"

t_done
