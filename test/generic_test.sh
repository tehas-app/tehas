#!/usr/bin/env bash
# Tests for scripts/check-generic.sh: a private address is reported in every
# file, and the list of names is data, used only when it is there and only
# for names. Each case runs the lint in a scratch repository.
# shellcheck disable=SC2016  # one case feeds the lint a line of shell, unexpanded
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1

# Built here, not written out: this file is linted too.
addr="100.$((60 + 40)).2.3"

repo=$T/repo
fresh() {
  rm -rf "$repo"
  mkdir -p "$repo/scripts" "$repo/bin" "$repo/docs"
  cp "$T_ROOT/scripts/check-generic.sh" "$repo/scripts/"
  git -C "$repo" init -q
  echo "echo hello" >"$repo/bin/tool"
}
lint="cd '$repo' && scripts/check-generic.sh"

t_case "a clean tree passes"
fresh
t_run "$lint"
t_eq "$t_rc" 0
t_contains "$t_out" "check-generic: ok"

t_case "a private address in a platform file is reported"
fresh
echo "ADDR=$addr" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:ADDR=$addr"

t_case "a documentation address and a longer number are not private addresses"
fresh
printf '%s\n' "ADDR=192.0.2.7" "VERSION=1$addr" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 0

t_case "every private range is an address, however it is written into a line"
tld=net
for a in "10.$((1 + 1)).3.4" "192.$((160 + 8)).1.1" "172.$((10 + 6)).0.1" "172.$((30 + 1)).9.9" "169.$((250 + 4)).1.1" \
  "100.$((60 + 4)).0.1" "100.$((120 + 7)).255.1" "host_$addr" "x=$addr:8080" "fd$((6 + 1))a:115c:a1e0::1" "node.tail1234.ts.$tld"; do
  fresh
  echo "see $a here" >"$repo/bin/tool"
  t_run "$lint"
  t_eq "$t_rc" 1
  t_contains "$t_out" "bin/tool:1:"
done

t_case "addresses just outside the private ranges, and versions, pass"
fresh
printf '%s\n' "172.$((30 + 2)).0.1" "100.$((120 + 8)).0.1" "100.$((60 + 3)).0.1" "8.8.8.8" "v1.10.$((1 + 1)).3.4" "127.0.0.1" "::1" "abcd:ef01" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 0

t_case "an address is reported in docs/ too: every commit is public"
fresh
echo "the host was at $addr" >"$repo/docs/log.md"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "docs/log.md:1:"

t_case "an address is found in a file that is not plain text"
fresh
printf 'ADDR=%s\0binary\n' "$addr" >"$repo/bin/blob"
t_run "$lint"
t_eq "$t_rc" 1

t_case "without a list of names only addresses are checked"
fresh
echo "# for the widgetshop product" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 0

t_case "a list of names adds words, and a path it allows may use them"
printf '%s\n' "# names of one installation" "widgetshop" "gizmo-[0-9]+" >"$repo/scripts/check-generic.names"
mkdir -p "$repo/notes"
echo "widgetshop went live" >"$repo/notes/log.md"
echo "gizmo-7" >"$repo/bin/other"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:# for the widgetshop product"
t_contains "$t_out" "bin/other:1:gizmo-7"
t_lacks "$t_out" "check-generic.names:"
t_contains "$t_out" "notes/log.md:1:"

t_case "the list can allow a path for names"
printf '%s\n' "widgetshop" "allow bin/tool" "allow notes/" >"$repo/scripts/check-generic.names"
rm "$repo/bin/other"
t_run "$lint"
t_eq "$t_rc" 0

t_case "the list cannot exempt a file from the address rule"
printf '%s\n' "widgetshop" "allow bin/" "allow " >"$repo/scripts/check-generic.names"
echo "ADDR=$addr" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:ADDR=$addr"

t_case "the list is data: a line of shell is refused, not run"
for line in 'exceptions+=("")' "words='x'" 'touch "$T/ran-names"' 'a;b'; do
  fresh
  printf '%s\n' "$line" >"$repo/scripts/check-generic.names"
  t_run "$lint"
  t_eq "$t_rc" 1
  t_contains "$t_out" "not a name or an allow line"
done
t_eq "$(find "$T" -maxdepth 1 -name 'ran-names' | wc -l)" 0

# Built so this file passes the lint it tests.
nm="acme"

t_case "a name in docs/ is reported"
fresh
echo "$nm" >"$repo/scripts/check-generic.names"
echo "$nm" >"$repo/docs/note.md"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "docs/note.md:1:"

t_case "an allow line still exempts a path"
fresh
printf '%s\n' "$nm" "allow docs/" >"$repo/scripts/check-generic.names"
echo "$nm" >"$repo/docs/note.md"
t_run "$lint"
t_eq "$t_rc" 0

t_case "without a names list the lint says so"
fresh
t_run "$lint"
t_eq "$t_rc" 0
t_contains "$t_out" "check-generic: no names list, names not checked"

t_case "a name list may hold anchors and groups"
fresh
printf '%s\n' '(^|[^a-z])zed([^m]|$)' >"$repo/scripts/check-generic.names"
echo "see zed here" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:see zed here"

# A repository with a base commit and two commits on top of it.
msgs() { # <subject of the second commit>
  fresh
  printf '%s\n' "$nm" >"$repo/scripts/check-generic.names"
  git -C "$repo" add -A
  git -C "$repo" -c user.name=t -c user.email=t@example.org commit -q -m base
  git -C "$repo" tag base
  echo one >"$repo/bin/one"
  git -C "$repo" add -A
  git -C "$repo" -c user.name=t -c user.email=t@example.org commit -q -m "first change"
  echo two >"$repo/bin/two"
  git -C "$repo" add -A
  git -C "$repo" -c user.name=t -c user.email=t@example.org commit -q -m "$1"
}
messages="cd '$repo' && "
msgcmd="scripts/check-generic.sh --messages base..HEAD"

t_case "--messages reports a name in a commit message"
msgs "fix $nm thing"
t_run "$messages$msgcmd"
t_eq "$t_rc" 1
t_contains "$t_out" "fix $nm thing"

t_case "--messages reports a name in the branch"
msgs "fix a thing"
t_run "${messages}GITHUB_HEAD_REF=feature/$nm-x $msgcmd"
t_eq "$t_rc" 1
t_contains "$t_out" "branch: feature/$nm-x"

t_case "--messages passes clean commits"
msgs "fix a thing"
t_run "${messages}GITHUB_HEAD_REF=feature/x $msgcmd"
t_eq "$t_rc" 0

t_case "--messages with a bad range exits 2, even without a names list"
fresh
t_run "cd '$repo' && scripts/check-generic.sh --messages bogus..HEAD"
t_eq "$t_rc" 2

t_case "--messages with a range that is an option is refused and not run"
fresh
t_run "cd '$repo' && scripts/check-generic.sh --messages --output=x"
t_eq "$t_rc" 2
t_eq "$(find "$repo" -maxdepth 1 -name x | wc -l)" 0

t_case "--messages without a range is refused"
fresh
t_run "cd '$repo' && scripts/check-generic.sh --messages"
t_eq "$t_rc" 2

t_case "a file whose path names a listed name is reported, whatever its content"
fresh
echo "$nm" >"$repo/scripts/check-generic.names"
echo "clean" >"$repo/docs/$nm-notes.md"
t_run "$lint"
t_eq "$t_rc" 1
t_contains "$t_out" "docs/$nm-notes.md: path names a listed name"

t_case "an allow line exempts a path from the path check"
fresh
printf '%s\n' "$nm" "allow docs/" >"$repo/scripts/check-generic.names"
echo "clean" >"$repo/docs/$nm-notes.md"
t_run "$lint"
t_eq "$t_rc" 0

t_case "a path that names nothing listed passes"
fresh
echo "$nm" >"$repo/scripts/check-generic.names"
echo "clean" >"$repo/docs/other.md"
t_run "$lint"
t_eq "$t_rc" 0

t_case "--names-only lets a private address pass, and without it the address fails"
fresh
echo "$nm" >"$repo/scripts/check-generic.names"
echo "ADDR=$addr" >"$repo/bin/tool"
t_run "$lint"
t_eq "$t_rc" 1
t_run "$lint --names-only"
t_eq "$t_rc" 0
t_contains "$t_out" "check-generic: ok"

t_case "--names-only still reports a listed name in content and in a path"
fresh
echo "$nm" >"$repo/scripts/check-generic.names"
echo "see $nm" >"$repo/bin/tool"
t_run "$lint --names-only"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/tool:1:see $nm"
fresh
echo "$nm" >"$repo/scripts/check-generic.names"
echo clean >"$repo/bin/$nm-x"
t_run "$lint --names-only"
t_eq "$t_rc" 1
t_contains "$t_out" "bin/$nm-x: path names a listed name"

t_case "--names-only without a names list says so and exits 0, addresses and all"
fresh
echo "ADDR=$addr" >"$repo/bin/tool"
t_run "$lint --names-only"
t_eq "$t_rc" 0
t_contains "$t_out" "check-generic: no names list, names not checked"

t_case "--names-only combines with --messages in either order"
msgs "fix $nm thing"
echo "ADDR=$addr" >"$repo/bin/tool"
t_run "${messages}$msgcmd --names-only"
t_eq "$t_rc" 1
t_contains "$t_out" "fix $nm thing"
t_lacks "$t_out" "$addr"
t_run "${messages}scripts/check-generic.sh --names-only --messages base..HEAD"
t_eq "$t_rc" 1
t_contains "$t_out" "fix $nm thing"
msgs "fix a thing"
echo "ADDR=$addr" >"$repo/bin/tool"
t_run "${messages}GITHUB_HEAD_REF=feature/x scripts/check-generic.sh --names-only --messages base..HEAD"
t_eq "$t_rc" 0
t_run "${messages}GITHUB_HEAD_REF=feature/x $msgcmd --names-only"
t_eq "$t_rc" 0

t_case "an unknown flag exits 2"
fresh
t_run "$lint --bogus"
t_eq "$t_rc" 2
t_run "$lint --names-only --bogus"
t_eq "$t_rc" 2

t_case "the list of names is ignored by git in this repository"
t_eq "$(git -C "$T_ROOT" check-ignore -q scripts/check-generic.names && echo ignored)" ignored

t_done
