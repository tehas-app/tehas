#!/usr/bin/env bash
# Tests for install.sh: the home it refuses before its first write, and the
# marker it leaves in a home it installs into.
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

# install.sh removes every file of docs/ and fun/ this checkout does not have,
# and the commands of an earlier layout, so a home that is not tehas's has to be
# refused before the first write. The loader takes TEHAS_HOME as it is, so a
# wrong one reaches this script as it was typed.
t_case "a relative TEHAS_HOME is refused and nothing under it changes"
fresh
instance
stray=$T/stray
rm -rf "$stray"
mkdir -p "$stray/tehas/docs"
echo "a page of someone else's" >"$stray/tehas/docs/index.md"
cd "$T" || exit 1
t_run "cd '$T' && TEHAS_HOME=tehas-home '$install'"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_HOME is not an absolute path: tehas-home"
t_contains "$t_out" "nothing was installed"
t_eq "$([ -e "$T/tehas-home" ] && echo there)" ""
t_eq "$(cat "$stray/tehas/docs/index.md")" "a page of someone else's"

t_case "--check reports a relative TEHAS_HOME as a line and exits 1"
t_run "cd '$T' && TEHAS_HOME=tehas-home '$install' --check"
t_eq "$t_rc" 1
# The three lines a home would be asked about say `not checked`, as they do for
# invalid settings: `tehas update --check` reads the `Home:` prefix and calls the
# refusal something an update does not fix, so the report has to say nothing else
# was looked at.
t_eq "$t_out" "Home:         REFUSED
  tehas: TEHAS_HOME is not an absolute path: tehas-home
Dependencies: not checked
Installed:    not checked
Tests:        not checked"

t_case "a path naming a parent directory is refused"
fresh
instance
rel=$T/rel
rm -rf "$rel"
mkdir -p "$rel/a/b"
t_run "cd '$rel/a' && TEHAS_HOME='$rel/a/b/..' '$install'"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_HOME names a parent directory: $rel/a/b/.."
t_eq "$([ -e "$rel/a/b" ] && echo there)" "there"
t_eq "$(find "$rel/a/b" -type f | wc -l)" 0

t_case "a non-empty directory with no marker is refused and its own files stay"
fresh
instance
other=$T/other-home
rm -rf "$other"
mkdir -p "$other/docs"
echo "not tehas's page" >"$other/docs/index.md"
echo "not tehas's either" >"$other/fun.txt"
t_run "TEHAS_HOME='$other' '$install'"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_HOME is not empty and holds no $install_marker: $other"
t_contains "$t_out" "nothing was installed"
t_eq "$(cat "$other/docs/index.md")" "not tehas's page"
t_eq "$(cat "$other/fun.txt")" "not tehas's either"

t_case "--check reports a non-empty home with no marker as a line and exits 1"
t_run "TEHAS_HOME='$other' '$install' --check"
t_eq "$t_rc" 1
t_eq "$t_out" "Home:         REFUSED
  tehas: TEHAS_HOME is not empty and holds no $install_marker: $other
Dependencies: not checked
Installed:    not checked
Tests:        not checked"
t_eq "$(cat "$other/docs/index.md")" "not tehas's page"

t_case "a home with the marker is installed into though it is not empty"
# The mark is the whole proof: nothing else under the home is questioned.
mkdir -p "$other/docs"
echo "a page this checkout does not have" >"$other/docs/gone.md"
: >"$other/$install_marker"
t_run "TEHAS_HOME='$other' '$install'"
t_eq "$t_rc" 0
t_eq "$([ -f "$other/libexec/dispatch" ] && echo there)" "there"
# The page of the home that this checkout does not have is the stale file it
# removes: with the mark there, this is a home it was asked to install under.
t_contains "$t_out" "removed: $other/docs/gone.md"

t_case "a home with a docs/ of its own and no marker is refused"
# The case the guard is for: a directory that is not tehas's, whose pages this
# run would have called stale and removed. Neither of the two ways an install of
# tehas marks its home is there.
fresh
instance
site=$T/not-tehas-home
rm -rf "$site"
mkdir -p "$site/docs" "$site/fun"
echo "the owner's page" >"$site/docs/index.md"
echo "the owner's front page" >"$site/fun/index.html"
t_run "TEHAS_HOME='$site' '$install'"
t_eq "$t_rc" 78
t_contains "$t_out" "TEHAS_HOME is not empty and holds no $install_marker: $site"
t_eq "$(cat "$site/docs/index.md")" "the owner's page"
t_eq "$(cat "$site/fun/index.html")" "the owner's front page"

t_case "a home whose name holds two dots is not read as naming a parent"
# The guard reads components, not substrings: `a..b` is someone's directory
# name, `..` between two slashes is the parent. The first is asked the question
# the second never has to answer, so the refusal must be the ordinary one and
# name no parent.
fresh
instance
dots=$T/a..b
rm -rf "$dots"
mkdir -p "$dots/docs"
echo "a page of someone else's" >"$dots/docs/index.md"
t_run "TEHAS_HOME='$dots' '$install'"
t_eq "$t_rc" 78
t_lacks "$t_out" "names a parent directory"
t_contains "$t_out" "TEHAS_HOME is not empty and holds no $install_marker: $dots"
t_eq "$(cat "$dots/docs/index.md")" "a page of someone else's"

t_case "a home prepared by bootstrap.sh user and not yet installed is accepted"
# `bootstrap.sh user` makes bin/, tools/, repos/ and tmp/ under the home before
# anyone has installed tehas there, and tools/ is where it puts node and the npm
# trees that env.sh puts on the PATH. Such a home has no marker yet and is still
# tehas's: nothing under it is removed and the run goes on and marks it.
fresh
instance
booted=$T/booted-home
rm -rf "$booted"
mkdir -p "$booted/bin" "$booted/tools/node/bin" "$booted/tools/npm/pi" "$booted/repos" "$booted/tmp/dl"
echo "#!/bin/sh" >"$booted/tools/node/bin/node"
t_run "TEHAS_HOME='$booted' '$install'"
t_eq "$t_rc" 0
t_eq "$([ -f "$booted/$install_marker" ] && echo there)" "there"
t_eq "$([ -f "$booted/tools/node/bin/node" ] && echo there)" "there"
t_eq "$([ -d "$booted/tools/npm/pi" ] && echo there)" "there"

t_case "a home with the sessions snapshot of tehasbot status is tehas's"
fresh
instance
snapped=$T/snapped-home
rm -rf "$snapped"
mkdir -p "$snapped/logs" "$snapped/mcp"
echo '{"api":1}' >"$snapped/mcp/sessions.jsonl"
echo '{"api":1,"by":"week"}' >"$snapped/mcp/outcomes.jsonl"
t_run "TEHAS_HOME='$snapped' '$install'"
t_eq "$t_rc" 0
t_eq "$([ -f "$snapped/$install_marker" ] && echo there)" "there"
t_eq "$(cat "$snapped/mcp/sessions.jsonl")" '{"api":1}'
t_eq "$(cat "$snapped/mcp/outcomes.jsonl")" '{"api":1,"by":"week"}'

t_case "the manual beside the front page is known as tehas's own, and gets the marker"
fresh
instance
manual=$T/manual-home
rm -rf "$manual"
mkdir -p "$manual/docs" "$manual/fun"
echo "# the manual of an earlier install" >"$manual/docs/host.md"
echo "<html></html>" >"$manual/fun/index.html"
t_run "TEHAS_HOME='$manual' '$install'"
t_eq "$t_rc" 0
t_eq "$([ -f "$manual/$install_marker" ] && echo there)" "there"
t_eq "$([ -f "$manual/docs/host.md" ] && echo there)" "there"

t_case "an existing home with no marker is known by its env.sh and dispatch, and gets one"
fresh
instance
legacy=$T/legacy-home
rm -rf "$legacy"
mkdir -p "$legacy/libexec"
echo "# the env of an earlier install" >"$legacy/env.sh"
echo "# the dispatcher of an earlier install" >"$legacy/libexec/dispatch"
t_run "TEHAS_HOME='$legacy' '$install'"
t_eq "$t_rc" 0
t_eq "$(head -1 "$legacy/env.sh")" "# shellcheck shell=bash"
t_eq "$([ -f "$legacy/$install_marker" ] && echo there)" "there"

t_case "a second run leaves the marker and reports no drift for it"
t_run "TEHAS_HOME='$legacy' '$install'"
t_eq "$t_rc" 0
t_lacks "$t_out" "$install_marker"

t_case "a real run puts the env.sh line at the top of ~/.bashrc once"
fresh
instance
# shellcheck disable=SC2016  # the line is the file's text, not to expand
printf '%s\n' '# the distribution'"'"'s file' '[ -z "$PS1" ] && return' >"$home/.bashrc"
t_run "$install"
t_eq "$t_rc" 0
t_eq "$(head -n 1 "$home/.bashrc")" "[ -f $th/env.sh ] && . $th/env.sh"
t_eq "$(sed -n 2p "$home/.bashrc")" "# the distribution's file"
t_contains "$t_out" "installed: the env.sh line at the top of $home/.bashrc"
t_run "$install"
t_eq "$(grep -c 'env.sh' "$home/.bashrc")" 1
t_lacks "$t_out" ".bashrc"

t_case "a ~/.bashrc that already sources env.sh, in the hand-written form, is left alone"
fresh
instance
printf '%s\n' 'alias ll="ls -l"' '[ -f ~/tehas/env.sh ] && . ~/tehas/env.sh' >"$home/.bashrc"
before=$(cat "$home/.bashrc")
t_run "TEHAS_HOME='$home/tehas' '$install'"
t_eq "$t_rc" 0
t_eq "$(cat "$home/.bashrc")" "$before"

t_case "a home with no ~/.bashrc gets one with the line"
fresh
instance
t_run "$install"
t_eq "$(cat "$home/.bashrc")" "[ -f $th/env.sh ] && . $th/env.sh"

t_case "--check says whether ~/.bashrc sources env.sh, and changes nothing"
t_run "$install --check"
t_contains "$t_out" "Shell:        OK"
echo 'alias ll="ls -l"' >"$home/.bashrc"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Shell:        MISSING ($home/.bashrc does not source $th/env.sh; ./install.sh puts the line at its top)"
t_eq "$(cat "$home/.bashrc")" 'alias ll="ls -l"'

t_done
