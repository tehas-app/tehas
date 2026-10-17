#!/usr/bin/env bash
# Tests for libexec/preview: forks and who opened a pull request, and
# reading every page of the open pull requests.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh"

t_case "a pull request from a fork is never built"
reset
a=$(commit feature)
pulls "$(pull 13 "$a" stranger/widget)"
t_run "$preview"
t_eq "$(builds)" 0
t_eq "$(runs)" 0
t_eq "$(count "$root" 13)" 0
t_contains "$t_out" "1 pull request from elsewhere, not built"

t_case "a pull request whose head repository is gone is never built"
reset
a=$(commit feature)
pulls "$(pull 14 "$a" "")"
t_run "$preview"
t_eq "$(builds)" 0

# Anyone can open a pull request between two branches a public repository
# already has. Its head is in the repository, and it is not built.
for who in NONE CONTRIBUTOR FIRST_TIME_CONTRIBUTOR FIRST_TIMER MANNEQUIN member Owner; do
  t_case "a pull request opened by '$who' between branches of the repository is never built"
  reset
  a=$(commit feature)
  pulls "$(pull 16 "$a" acme/widget "$who")"
  t_run "$preview"
  t_eq "$t_rc" 0
  t_eq "$(builds)" 0
  t_eq "$(runs)" 0
  t_eq "$(count "$root" 16)" 0
  t_contains "$t_out" "1 pull request opened by someone outside the repository, not built"
  t_lacks "$t_out" "from elsewhere"
  t_eq "$(writes)" ""
done

# A list that does not say who opened a pull request is not a clear answer.
# The sweep keeps on it, and so does the run: nothing is built and nothing
# that serves is removed.
for shape in 'del(.author_association)' '.author_association = null' '.author_association = ["OWNER"]' '.author_association = true'; do
  t_case "a list that does not say who opened a pull request builds and removes nothing ($shape)"
  reset
  a=$(commit feature)
  pulls "$(pull 12 "$a")" "$(pull 16 "$a")"
  t_run "$preview"
  t_eq "$(state_of 12) $(state_of 16)" "serving serving"
  b=$(commit other)
  pulls "$(pull 17 "$b")" "$(pull 16 "$a" | jq -c "$shape")"
  rm -f "$T"/calls/*
  t_run "$preview"
  t_eq "$t_rc" 0
  t_contains "$t_out" "the list of open pull requests does not say who opened each; nothing was built or removed"
  t_eq "$(builds)" 0
  t_eq "$(t_calls docker | grep -c '^rm ')" 0
  t_eq "$(state_of 12) $(state_of 16)" "serving serving"
  t_eq "$(count "$root" 17)" 0
  t_eq "$(jq -r '(now - (.generatedAt | fromdateiso8601)) < 120' "$live/previews.json")" "true"
done

t_case "a fork that does not say who opened it stops nothing"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")" "$(pull 26 "$a" stranger/widget "")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_lacks "$t_out" "does not say who"

for who in OWNER MEMBER COLLABORATOR; do
  t_case "a pull request opened by '$who' is built"
  reset
  a=$(commit feature)
  pulls "$(pull 16 "$a" acme/widget "$who")"
  t_run "$preview"
  t_eq "$(state_of 16)" "serving"
  t_lacks "$t_out" "outside the repository"
done

# The list is read in pages of 100. Only the newest page was asked for at
# first: a hundred newer pull requests from forks push every one of ours off
# it, and the run then finds none of them open and removes every preview.
t_case "a run reads every page of open pull requests, not the newest one only"
reset
a=$(commit feature)
pull 12 "$a"
page 1 "$(page_of 100 101 stranger/widget NONE)"
jq -s . "$fx/pull-12.json" >"$fx/pulls-2.json"
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(runs)" 1
t_contains "$t_out" "100 pull requests from elsewhere, not built"
t_eq "$(t_calls gh | grep -c 'pulls?')" 2

t_case "a list longer than the run reads builds and removes nothing"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
for p in 1 2 3 4 5 6 7 8 9 10; do
  page "$p" "$(page_of 100 $((1001 + (p - 1) * 100)) stranger/widget NONE)"
done
# The tenth page is full, so the run asks for one more to know where the list
# ends. This one has a pull request in it: the list goes past what was read.
page 11 "$(page_of 30 2001 stranger/widget NONE)"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "the open pull requests are more than 1000: the list was not read, nothing was built or removed"
t_eq "$(t_calls gh | grep -c 'pulls?')" 11
t_eq "$(builds)" 0
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_lacks "$t_out" "no longer open, removed"
t_eq "$(jq -r '(now - (.generatedAt | fromdateiso8601)) < 120' "$live/previews.json")" "true"

# A thousand open pull requests is the last list that fits in ten pages. The
# eleventh answer is empty, which is where GitHub ends: the list was read, and
# the run goes on with what it read.
t_case "a thousand open pull requests is a list that was read"
reset
a=$(commit feature)
pull 12 "$a"
for p in 1 2 3 4 5 6 7 8 9; do
  page "$p" "$(page_of 100 $((101 + (p - 1) * 100)) stranger/widget NONE)"
done
page_with 10 "$(page_of 99 1001 stranger/widget NONE)" "$(pull 12 "$a")"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(t_calls gh | grep -c 'pulls?')" 11
t_lacks "$t_out" "more than 1000"
t_contains "$t_out" "999 pull requests from elsewhere, not built"
t_eq "$(state_of 12)" "serving"
t_eq "$(runs)" 1

t_case "pull requests by people outside the repository take no place from the others"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=2
a=$(commit one)
b=$(commit two)
pulls "$(pull 29 "$a" acme/widget NONE)" "$(pull 28 "$a" acme/widget NONE)" "$(pull 27 "$b" acme/widget CONTRIBUTOR)" \
  "$(pull 26 "$a" stranger/widget NONE)" "$(pull 22 "$b")" "$(pull 21 "$a" acme/widget COLLABORATOR)"
t_run "$preview"
t_eq "$(state_of 22) $(state_of 21)" "serving serving"
t_eq "$(runs)" 2
t_eq "$(count "$root" '2[6-9]')" 0
# A fork is counted as a fork, whoever opened it, and the rest in a line of their own.
t_contains "$t_out" "1 pull request from elsewhere, not built"
t_contains "$t_out" "3 pull requests opened by someone outside the repository, not built"

# Each page is an answer of its own, taken at its own moment: a pull request
# can fall between two of them and be in neither. The run asks GitHub about one
# it means to remove before it removes anything.
t_case "a pull request that fell between two pages keeps its preview"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
page 1 "$(page_of 100 101 stranger/widget NONE)"
page 2 "$(page_of 40 201 stranger/widget NONE)"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "#12: open though the list did not say so, kept"
t_lacks "$t_out" "no longer open, removed"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_eq "$(count "$root" 12)" 1
t_eq "$(count "$fx/docker" '*-12')" 2

t_case "nothing is removed when GitHub does not answer about that pull request"
reset
a=$(commit feature)
b=$(commit other)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_eq "$(state_of 12)" "serving"
pulls "$(pull 13 "$b")"
: >"$fx/pull-12.rc"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_contains "$t_out" "#12: no answer from GitHub, kept"
t_lacks "$t_out" "no longer open, removed"
t_eq "$(state_of 12)" "serving"
t_eq "$(cat "$fx/docker/container-12")" "$a"
t_eq "$(count "$fx/docker" '*-12')" 2

t_case "a pull request that comes back on two pages takes no place from another"
reset
t_instance "${T_MINIMAL[@]}" TEHAS_PREVIEWS=1 TEHAS_DOMAIN=example.org TEHAS_PREVIEW_MAX=2
a=$(commit one)
b=$(commit two)
# The list moves while it is read: #12 is at the end of page 1 and again at the
# start of page 2. Counted twice it would push #11 over the limit.
page_with 1 "$(page_of 99 101 stranger/widget NONE)" "$(pull 12 "$a")"
page_with 2 "[]" "$(pull 12 "$a")" "$(pull 11 "$b")"
rm -f "$T"/calls/*
t_run "$preview"
t_eq "$t_rc" 0
t_eq "$(state_of 12) $(state_of 11)" "serving serving"
t_eq "$(runs)" 2
t_lacks "$t_out" "over-limit"
t_contains "$t_out" "99 pull requests from elsewhere, not built"

t_case "a preview whose pull request turns out to be an outsider's is removed at the next run"
reset
a=$(commit feature)
pulls "$(pull 16 "$a")"
t_run "$preview"
t_eq "$(state_of 16)" "serving"
pulls "$(pull 16 "$a" acme/widget NONE)"
t_run "$preview"
t_contains "$t_out" "#16: it was not opened by someone of acme/widget, removed"
t_eq "$(count "$fx/docker" '*-16')" 0
t_eq "$(count "$root" 16)" 0

t_case "a head that is not a commit id is never built"
reset
pulls '{"number":15,"title":"x","html_url":"u","author_association":"MEMBER","head":{"sha":"main; id","repo":{"full_name":"acme/widget"}}}'
t_run "$preview"
t_eq "$(builds)" 0

t_done
