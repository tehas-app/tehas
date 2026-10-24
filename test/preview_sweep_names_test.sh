#!/usr/bin/env bash
# Tests for libexec/preview --sweep: unclear answers, container names and
# labels, the repository check, volumes, and its temporary file.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/preview_lib.sh
. "$(dirname "$0")/preview_lib.sh" || exit 1

# unclear <number> <body>: a leftover of that number, and an answer of exit 0 with that body.
unclear() {
  left "$1" "00000000000000000000000000000000000000$1"
  printf '%s\n' "$2" >"$fx/pull-$1.json"
  t_run "$preview --sweep"
  t_eq "$(count "$fx/docker" "*-$1")" 2
  t_eq "$(count "$root" "$1")" 1
  t_contains "$t_out" "#$1: no answer from GitHub, kept"
}

t_case "sweep: an answer of {} keeps everything"
reset
unclear 47 '{}'

t_case "sweep: a state that is neither open nor closed keeps everything"
reset
unclear 48 '{"number":48,"state":"weird","head":{"repo":{"full_name":"acme/widget"}}}'

t_case "sweep: an open answer with no head keeps everything"
reset
unclear 49 '{"number":49,"state":"open"}'

t_case "sweep: an answer about another pull request keeps everything"
reset
unclear 50 '{"number":51,"state":"closed"}'

t_case "sweep: a state that is not text keeps everything"
reset
unclear 53 '{"number":53,"state":true}'

t_case "sweep: an open pull request whose fork was deleted is removed"
reset
left 54 0000000000000000000000000000000000000054
echo '{"number":54,"state":"open","head":{"sha":"x","repo":null}}' >"$fx/pull-54.json"
t_run "$preview --sweep"
t_eq "$(count "$fx/docker" '*-54')" 0
t_contains "$t_out" "#54: swept, its branch is not in acme/widget"

t_case "sweep: a container under another name goes with its closed pull request, once"
reset
a=$(commit one)
pull 63 "$a" >/dev/null
jq '.state = "closed"' "$fx/pull-63.json" >"$fx/x" && mv "$fx/x" "$fx/pull-63.json"
echo "c0de03 63 acme/widget debug" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "rm -f -v c0de03"
t_contains "$t_out" "#63: swept, the pull request is closed"
t_run "$preview --sweep"
t_lacks "$t_out" "swept"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0

t_case "sweep: a second container of an open pull request is removed by id, the serving one stays"
reset
a=$(commit one)
pulls "$(pull 64 "$a")"
t_run "$preview"
echo "c0de04 64 acme/widget extra" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "rm -f -v c0de04"
t_contains "$t_out" "container c0de04: swept, a second container of #64"
t_eq "$(cat "$fx/docker/container-64")" "$a"
t_lacks "$(t_calls docker)" "rm -f -v ab64"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 1

t_case "sweep: the serving container is kept when Docker gives it several names"
reset
a=$(commit one)
pulls "$(pull 64 "$a")"
t_run "$preview"
for names in "tehas-preview-64,proxy/app" "proxy/app,tehas-preview-64" "proxy/a,tehas-preview-64,proxy/b"; do
  echo "$names" >"$fx/names-64"
  t_run "$preview --sweep"
  t_eq "$(cat "$fx/docker/container-64" 2>/dev/null)" "$a"
  t_lacks "$t_out" "swept"
done
t_lacks "$(t_calls docker)" "rm -f -v ab64"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0

t_case "sweep: a container whose name column is empty is kept"
: >"$fx/names-64"
echo "c0de06 64 acme/widget " >"$fx/extra-containers"
t_run "$preview --sweep"
t_eq "$(cat "$fx/docker/container-64" 2>/dev/null)" "$a"
t_lacks "$(t_calls docker)" "rm -f -v ab64"
t_lacks "$(t_calls docker)" "rm -f -v c0de06"
t_lacks "$t_out" "swept"

t_case "sweep: a name that only has the serving name inside it is a second container"
rm -f "$fx/names-64"
printf '%s\n' "c0de07 64 acme/widget tehas-preview-640" "c0de08 64 acme/widget x,old-tehas-preview-64" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$(t_calls docker)" "rm -f -v c0de07"
t_contains "$(t_calls docker)" "rm -f -v c0de08"
t_eq "$(cat "$fx/docker/container-64")" "$a"

t_case "sweep: a closed pull request with a container that will not go is not called swept, and is tried again"
reset
left 66 0000000000000000000000000000000000000066
pull 66 0000000000000000000000000000000000000066 >/dev/null 2>&1
jq '.state = "closed"' "$fx/pull-66.json" >"$fx/x" && mv "$fx/x" "$fx/pull-66.json"
echo "c0de09 66 acme/widget debug" >"$fx/extra-containers"
echo c0de09 >"$fx/rm-fails"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_contains "$t_out" "#66: a container could not be removed, tried again at the next sweep"
t_lacks "$t_out" "#66: swept"
t_eq "$(count "$root" 66)" 1
t_eq "$(count "$fx/docker" image-66)" 1
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0
rm -f "$fx/rm-fails"
t_run "$preview --sweep"
t_contains "$t_out" "#66: swept, the pull request is closed"
t_eq "$(count "$root" 66)" 0
t_eq "$(count "$fx/docker" '*-66')" 0
t_eq "$(jq -r '.removed' "$root/sweep.json")" 1

t_case "sweep: a repository GitHub does not show keeps every pull request, and the label passes still run"
reset
left 52 0000000000000000000000000000000000000052
echo "c0de01 7 other/thing foreign" >"$fx/extra-containers"
: >"$fx/repo-fails"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$(count "$fx/docker" '*-52')" 2
t_contains "$t_out" "GitHub does not show acme/widget; nothing was swept by its word"
t_contains "$(t_calls docker)" "rm -f -v c0de01"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 1

# An answer of exit 0 is the repository only when it says so itself.
for body in '[]' '"acme/widget"' '{}' '{"full_name":null}' '{"full_name":"other/thing"}' '{"full_name":"acme/widget-two"}' ''; do
  t_case "sweep: a repository answer of '$body' keeps every pull request"
  reset
  left 52 0000000000000000000000000000000000000052
  printf '%s' "$body" >"$fx/repo-body"
  t_run "$preview --sweep"
  t_eq "$t_rc" 0
  t_eq "$(count "$fx/docker" '*-52')" 2
  t_eq "$(count "$root" 52)" 1
  t_contains "$t_out" "GitHub does not show acme/widget; nothing was swept by its word"
  t_eq "$(t_calls gh | grep -c 'pulls/52')" 0
  t_eq "$(jq -r '.removed' "$root/sweep.json")" 0
done

t_case "sweep: a label that writes the repository with other capitals is this repository's"
reset
a=$(commit one)
b=$(commit two)
pulls "$(pull 71 "$a")" "$(pull 72 "$b")"
t_run "$preview"
echo "Acme/Widget" >"$fx/label-repo"
printf '%s\n' "$(cat "$fx/docker/image-71") ACME/widget" "$(cat "$fx/docker/image-72") ACME/widget" >"$fx/image-repos"
rm -f "$T"/calls/docker* "$T"/calls/gh
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_lacks "$t_out" "swept"
t_eq "$(cat "$fx/docker/container-71") $(cat "$fx/docker/container-72")" "$a $b"
t_eq "$(count "$fx/docker" 'image-7*')" 2
t_eq "$(t_calls docker | grep -c '^rm \|^image rm ')" 0
t_eq "$(jq -r '.removed' "$root/sweep.json")" 0
# Its pull requests are asked about, so a closed one goes and a second container is seen.
t_eq "$(t_calls gh | grep -c 'pulls/7[12]$')" 2
jq '.state = "closed"' "$fx/pull-71.json" >"$fx/x" && mv "$fx/x" "$fx/pull-71.json"
echo "c0de0a 72 ACME/WIDGET extra" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$t_out" "#71: swept, the pull request is closed"
t_contains "$t_out" "container c0de0a: swept, a second container of #72"
t_eq "$(count "$fx/docker" '*-71')" 0
t_eq "$(cat "$fx/docker/container-72")" "$b"
t_eq "$(jq -r '.removed' "$root/sweep.json")" 2

t_case "sweep: a label of a repository that only starts the same is still another's"
reset
left 73 0000000000000000000000000000000000000073
echo "acme/widget-two" >"$fx/label-repo"
t_run "$preview --sweep"
t_contains "$t_out" "container ab73: swept, it belongs to no pull request of acme/widget"

t_case "every container is removed with its volumes: a replace, a stop, a purge and a sweep"
reset
a=$(commit one)
b=$(commit two)
pulls "$(pull 81 "$a")" "$(pull 82 "$b")"
t_run "$preview"
a2=$(commit one)
pulls "$(pull 81 "$a2")" "$(pull 82 "$b")"
t_run "$preview"
# 82 is merged: the run purges it.
pulls "$(pull 81 "$a2")"
t_run "$preview"
# 81 stops within its five seconds at a new commit: the settle path.
a3=$(commit one)
pulls "$(pull 81 "$a3")"
echo "false 0" >"$fx/settle"
t_run "$preview"
rm -f "$fx/settle"
# The sweep: a foreign container, a second container of an open pull request,
# and what a closed one left.
pulls "$(pull 81 "$a3")"
t_run "$preview"
left 83 0000000000000000000000000000000000000083
pull 83 0000000000000000000000000000000000000083 >/dev/null 2>&1
jq '.state = "closed"' "$fx/pull-83.json" >"$fx/x" && mv "$fx/x" "$fx/pull-83.json"
printf '%s\n' "c0de01 7 other/thing foreign" "c0de02 81 acme/widget extra" "c0de03 83 acme/widget debug" >"$fx/extra-containers"
t_run "$preview --sweep"
t_contains "$t_out" "#83: swept, the pull request is closed"
removals=$(t_calls docker | grep '^rm ')
for of in tehas-preview-81 tehas-preview-82 tehas-preview-83 c0de01 c0de02 c0de03; do
  t_contains "$removals" "rm -f -v $of"
done
t_eq "$(printf '%s\n' "$removals" | grep -c -v '^rm -f -v [0-9a-z-]*$')" 0

t_case "a notice GitHub did not take leaves the preview serving, and is asked again"
reset
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_stub gh 'case "$1 $2" in
  "api repos/acme/widget/pulls?"*) cat "$T/fx/pulls.json" ;;
  "api repos/acme/widget/pulls/"*) cat "$T/fx/pull-$2.json" ;;
  "api -X") exit 1 ;;
esac'
t_run "$preview"
t_eq "$(state_of 12)" "serving"
t_eq "$(count "$root/12" .notice)" 0

t_case "the sweep leaves no temporary file of its own behind"
reset
stub_gh
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
t_run "$preview --sweep"
t_eq "$t_rc" 0
t_eq "$([ -e "$root/sweep.json" ] && echo yes || echo no)" "yes"
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0

# The sweep's temporary file was removed on the last line of the sweep, which a
# signal never reaches. The trap removes what this run made. The sweep is stopped
# while it waits on the call about one pull request: fx/slow holds that call
# until fx/go is there, and fx/held is what the wait looks for. Not the file
# under tmp: it is there a moment before the run remembers it, and a signal in
# that moment leaves it behind, which a loaded runner hit.
t_case "a sweep that is stopped partway leaves no temporary file either"
reset
stub_gh
a=$(commit feature)
pulls "$(pull 12 "$a")"
t_run "$preview"
: >"$fx/slow"
rm -f "$T"/calls/*
t_run "$preview --sweep & sweep=\$!
for _ in \$(seq 200); do
  [ ! -e \"$fx/held\" ] || break
  /bin/sleep 0.05
done
kill -TERM \"\$sweep\" 2>/dev/null
: >\"$fx/go\"
wait \"\$sweep\" 2>/dev/null
printf 'rc=%s\n' \"\$?\""
rm -f "$fx/slow" "$fx/go" "$fx/held"
t_contains "$t_out" "rc=143"
t_eq "$(count "$root" sweep.json)" 0
t_eq "$(find "$T/home/tehas/tmp" -mindepth 1 2>/dev/null | wc -l)" 0
t_eq "$(state_of 12)" "serving"

t_case "the run closes the lock descriptor bash gave it, not a number it guessed"
t_eq "$(grep -c -F '9>&-' "$T_ROOT/libexec/preview")" 0
t_contains "$(cat "$T_ROOT/libexec/preview")" '{edge_lock_fd}>&-'


t_done
