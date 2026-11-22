#!/usr/bin/env bash
# Tests for install.sh's first install: labels it could not make, the account
# ids it records, and the fixture every install of these files runs.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
real_console

t_case "labels that cannot be made leave the first install owed, and the next run does it"
fresh
instance
t_stub gh '[ "$1" != label ] || exit 1'
t_run "$install"
t_eq "$t_rc" 0
t_contains "$t_out" "later: ./install.sh again   (the first install's labels or units are not done: tehas labels failed)"
t_eq "$([ -e "$th/state/first-install" ] && echo owed)" "owed"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "First install: NOT DONE (labels and core units; ./install.sh does them)"
t_stub gh
rm -f "$T"/calls/*
t_run "$install"
t_contains "$t_out" "first install: labels made in acme/widget"
t_eq "$([ -e "$th/state/first-install" ] && echo owed)" ""
t_contains "$(t_calls systemctl)" "--user enable --now tehas-dispatch.timer tehas-dispatch.path"

t_case "an id that is not the recorded one stops the install with 78 and names both"
fresh
instance
t_run "$install"
t_eq "$t_rc" 0
echo "alice 7" >"$th/state/account-ids"
rm -f "$T"/calls/*
t_run "$install"
t_eq "$t_rc" 78
t_contains "$t_out" "the login alice now has the user id $(t_uid alice), not the recorded 7"
t_contains "$t_out" "tehas update --accept-id alice"
t_eq "$(cat "$th/state/account-ids")" "alice 7"
t_eq "$(t_calls systemctl)" ""

t_case "a first install with an app and no GH_TOKEN records the ids through the app's token"
# The shim is not in place before the ids are recorded, so gh (signed out
# without GH_TOKEN, exit 4) has only the token made from the app's key (#1714).
fresh
instance TEHAS_GITHUB_APP_ID=5452597
always=("${T_ENV_ALWAYS[@]}")
T_ENV_ALWAYS=()
for w in "${always[@]}"; do [[ $w == GH_TOKEN=* ]] || T_ENV_ALWAYS+=("$w"); done
mkdir -p "$home/.config/tehas" && printf 'a key\n' >"$home/.config/tehas/github-app.pem"
t_stub openssl '[ "$1" != dgst ] || { cat >/dev/null; printf signature; }'
t_stub curl 'cat >/dev/null; case "$*" in *access_tokens*) printf "{\"token\":\"ghs_minted\",\"expires_at\":\"%s\"}" "$(date -u -d "+1 hour" +%FT%TZ)" ;; */installation*) printf "{\"id\":174742089}" ;; */app*) printf "{\"slug\":\"tehas-app-bot\"}" ;; esac'
t_stub gh '[ -n "${GH_TOKEN:-}" ] || { echo "gh: not logged in" >&2; exit 4; }
[ "$1 $2" != "api users/alice" ] || printf "%s\n" "$GH_TOKEN" >"$T/gh-token"'
rm -f "$T/gh-token"
t_run "$install"
t_eq "$(cat "$th/state/account-ids")" "alice $(t_uid alice)"
t_eq "$(cat "$T/gh-token")" "ghs_minted"
t_eq "$t_rc" 0
t_lacks "$t_out" "ghs_minted"
t_lacks "$t_out" "nothing was installed"
t_stub gh
T_ENV_ALWAYS=("${always[@]}")
rm -f "$T/bin/openssl" "$T/bin/curl"

# The fixture of install_lib.sh is what every install runs, so this case plants
# in a checkout of its own: the test files of a run read $T_ROOT in parallel.
t_case "a fixture is the checkout's tracked files, and no file beside them"
fresh
instance
planted=$T/planted
rm -rf "$planted"
cp -R "$fixture" "$planted"
git init -q -b main "$planted"
git -C "$planted" -c user.name=t -c user.email=t@example.com add -A
git -C "$planted" -c user.name=t -c user.email=t@example.com commit -q -m fixture
# COMMIT and VERSION are the build's own output, so the tree the build reads
# holds neither: left in, a run that copied the tree and wrote nothing would
# answer for them.
rm -f "$planted/COMMIT" "$planted/VERSION"
# Two files nobody tracks, and no ignore rule that hides them: a plain copy of
# the tree would take them and an install would put them on the host.
echo '# nobody tracks this page' >"$planted/host/docs/zz-untracked.md"
echo '# nobody ships this' >"$planted/lib/polyfill-zz.sh"
# An edit to a tracked file, left uncommitted: the fixture is the working tree,
# so this is what the next case installs.
echo '# edited and not committed' >>"$planted/lib/paths.sh"
planted_fixture=$T/planted-fixture
install_fixture "$planted" "$planted_fixture"
t_eq "$([ -e "$planted_fixture/host/docs/zz-untracked.md" ] && echo there)" ""
t_eq "$([ -e "$planted_fixture/lib/polyfill-zz.sh" ] && echo there)" ""
t_eq "$(cat "$planted_fixture/lib/paths.sh")" "$(cat "$planted/lib/paths.sh")"
# COMMIT and VERSION name the tree with no git the way install.sh would have
# read them from the checkout: neither is in the tree it was built from, so
# these two lines answer for the build rather than for the copy.
t_eq "$([ -e "$planted/COMMIT" ] || [ -e "$planted/VERSION" ] && echo there)" ""
t_eq "$(cat "$planted_fixture/COMMIT")" "$(git -C "$planted" rev-parse HEAD)"
t_eq "$(cat "$planted_fixture/VERSION")" "$(git -C "$planted" rev-parse --short=7 HEAD)"
t_run "$planted_fixture/install.sh"
t_eq "$t_rc" 0
t_eq "$(cd "$home" && find . -name 'zz-untracked.md' -o -name 'polyfill-zz.sh')" ""
t_eq "$(install_list_report "$(sorted "$files")" "$(core_files)")" ""
# The same tree with one tracked file taken out of the working tree: the build
# leaves it out and still exits, so a file a person is mid-way through deleting
# does not stop the fixture.
rm "$planted_fixture/lib/paths.sh" "$planted/lib/paths.sh"
install_fixture "$planted" "$planted_fixture"
t_eq "$t_rc" 0
t_eq "$([ -e "$planted_fixture/lib/paths.sh" ] && echo there)" ""

t_done
