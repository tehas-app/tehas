#!/usr/bin/env bash
# Tests for install.sh on a home a fresh install made: the manual and the front
# page it puts, a second run, and what --check and --check --tests report.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1
real_console

t_case "--check makes no network"
fresh
instance
t_run "$install"
rm -f "$T/calls/docker"
t_run "$install --check"
t_eq "$(t_calls docker)" ""

t_case "the manual and the front page are installed"
t_eq "$(cd "$th/docs" && find . -type f ! -name version.txt | wc -l)" "$(find "$fixture/host/docs" -type f | wc -l)"
# The front page is the stage console update assets made: the checkout's files
# and a hashed copy of each script and stylesheet a page names by tag. The count
# is the stage's own, so neither an install nor a stage that hashed nothing passes.
"$T_CONSOLE" update assets "$fixture/host" "$T/stage-count" ||
  t_bad "console update assets could not stage $fixture/host"
t_eq "$(cd "$th/fun" && find . -type f ! -name version.txt | wc -l)" \
  "$(cd "$T/stage-count/fun" && find . -type f | wc -l)"
# The checkout's files, each of them still there but the .d.ts type files,
# which no page loads, and beside them the hashed copies and nothing else. Both
# halves are read out of the home, where the install put them: only the home
# proves that both came.
t_eq "$(cd "$th/fun" && find . -type f ! -name version.txt -regextype posix-extended \
      ! -regex '.*/[A-Za-z0-9_.-]+\.[0-9a-f]{12}\.(js|css)' | LC_ALL=C sort)" \
  "$(cd "$fixture/host/fun" && find . -type f ! -name '*.d.ts' | LC_ALL=C sort)"
t_eq "$(cd "$th/fun" && find . -name '*.d.ts')" ""
# Two pages name some of these files, so the copies are fewer than the tags.
t_eq "$(cd "$th/fun" && find . -type f -regextype posix-extended \
      -regex '.*/[A-Za-z0-9_.-]+\.[0-9a-f]{12}\.(js|css)' | wc -l)" 24

t_case "a tag of a dashboard page names the hashed copy, which is installed"
# The name holds the first 12 hex digits of the sha256 of the file's bytes, the
# same 12 the docs server treats as immutable, and the page asks for that name
# and no other.
for name in panels.js panels.css; do
  stem=${name%%.*}
  extension=${name##*.}
  hash=$(sha256sum "$fixture/host/fun/$name" | cut -c1-12)
  t_contains "$(cat "$th/fun/index.html")" "/fun/$stem.$hash.$extension"
  t_eq "$([ -f "$th/fun/$stem.$hash.$extension" ] && echo there)" "there"
  # The copy holds the bytes the checkout holds, under the new name.
  t_eq "$(cmp -s "$fixture/host/fun/$name" "$th/fun/$stem.$hash.$extension" && echo same)" "same"
  # And the page asks for no other name of it.
  t_lacks "$(cat "$th/fun/index.html")" "src=\"/fun/$name\""
done
# The theme a template names keeps its name: nothing here can hash a name the
# request makes, and the file is installed as it is.
t_contains "$(cat "$th/fun/index.html")" 'href="/fun/theme-{{ $design }}.css"'
t_eq "$(cd "$th/fun" && find . -name 'theme-*.css' | wc -l)" 2

t_case "a hashed name of older content is removed, and --check says so"
# An earlier install's name, of bytes the checkout no longer has: it is stale by
# the same rule as a page that went away, and a real run takes it back.
echo "// the panel as it was" >"$th/fun/panels.0123456789ab.js"
echo "// older still" >"$th/fun/panels.0123456789ab.js.prev"
t_run "$install --check"
t_contains "$t_out" "panels.0123456789ab.js"
t_run "$install"
t_eq "$([ -e "$th/fun/panels.0123456789ab.js" ] && echo there)" ""
t_eq "$([ -e "$th/fun/panels.0123456789ab.js.prev" ] && echo there)" ""
t_eq "$(cd "$th/fun" && find . -name '*.new' | wc -l)" 0

t_case "the installed pages are the stage, byte for byte"
t_run "'$T_CONSOLE' update assets '$fixture/host' '$T/stage-same'"
t_eq "$t_rc" 0
# The home holds the stage and nothing of its own beside the version file an
# install writes there.
for dir in docs fun; do
  t_eq "$(cd "$th/$dir" && find . -type f ! -name version.txt | LC_ALL=C sort)" \
    "$(cd "$T/stage-same/$dir" && find . -type f | LC_ALL=C sort)"
done

t_case "the instance's model list is what Pi gets, with the keys it left out filled"
# By parts, so key order is no part of the answer.
t_eq "$(jq -c '[.providers[].models[].id]' "$home/.pi/agent/models.json")" \
  "$(jq -c '[.providers[].models[].id]' "$TEHAS_INSTANCE_DIR/models.json")"
t_eq "$(jq -c '[.providers[].models[] | .name == .id] | unique' "$home/.pi/agent/models.json")" '[true]'
t_eq "$(jq -c '[.providers[].models[] | .input, .reasoning] | unique' "$home/.pi/agent/models.json")" '[false,["text"]]'

t_case "nothing installed names a file that is absent"
missing=""
for unit in "$home"/.config/systemd/user/*; do
  while read -r ref; do
    # The path unit watches for a file a cycle leaves and a poll removes; the
    # fetch socket is made by systemd when tehas mcp serve starts it.
    case "$ref" in */logs/* | */state/dispatch.again) ref=$(dirname "$ref") ;; */run/mcp-fetch/fetch.sock) continue ;; esac
    [ -e "$ref" ] || missing="$missing $ref"
  done < <(grep -o '%h/tehas/[A-Za-z0-9_./-]*' "$unit" | sed "s|%h|$home|")
done
for file in docs.compose.yaml router.compose.yaml; do
  for ref in $(grep -o '"\.\.\?/[A-Za-z0-9_./-]*:' "$th/edge/$file" | tr -d '":'); do
    [ -e "$th/edge/$ref" ] || missing="$missing $ref"
  done
done
t_eq "$missing" ""
t_case "a second run changes nothing"
t_run "$install"
t_contains "$t_out" "tehas is up to date"

t_case "--check on a matching install reports six lines and exits 0"
# The follow-ups of the install above are taken as done, which is what running
# tehas update does to the record. A host that still owes one is another case.
rm -f "$th/state/update-pending"
t_run "$install --check"
t_eq "$t_rc" 0
t_eq "$t_out" "Instance:     $T/instance OK
Dependencies: not checked (running inside the tests)
Slice:        not checked (running inside the tests)
Installed:    OK
Shell:        OK
Tests:        not run (./install.sh --check --tests runs them)"

t_case "--check --tests runs the suite and reports it"
# A stand-in for the suite: these tests never start the real one, which would
# run them again.
printf '#!/usr/bin/env bash\n: >"%s/suite-ran"\nexit "$(cat "%s/suite-rc")"\n' "$T" "$T" >"$T/bin/suite"
chmod +x "$T/bin/suite"
echo 0 >"$T/suite-rc"
t_env TEHAS_TEST_SUITE="$T/bin/suite"
rm -f "$T/suite-ran"
t_run "$install --check"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/suite-ran" ] && echo ran)" ""
t_run "$install --check --tests"
t_eq "$t_rc" 0
t_eq "$([ -e "$T/suite-ran" ] && echo ran)" ran
t_contains "$t_out" "Tests:        OK"

t_case "--check --tests with a failing suite says FAILED and exits 1"
echo 1 >"$T/suite-rc"
t_run "$install --check --tests"
t_eq "$t_rc" 1
t_contains "$t_out" "Tests:        FAILED"

t_case "--check --tests inside the tests with no stand-in runs nothing"
t_env
rm -f "$T/suite-ran"
t_run "$install --check --tests"
t_eq "$t_rc" 0
t_contains "$t_out" "Tests:        not checked (running inside the tests)"
t_eq "$([ -e "$T/suite-ran" ] && echo ran)" ""

t_case "--check proves the app's cycle token from the product checkout, and a refused push exits 1"
# The installed tehasbot gives the kept token and login of the app's world. git
# is real but for the proof's push, which records its token and is refused
# while $T/push-refused holds the remote's words.
t_app_world
real_git=$(command -v git)
product=$th/repos/widget
"$real_git" init -q --initial-branch=main "$product"
"$real_git" -C "$product" -c user.name=t -c user.email=t@example.com commit -q --allow-empty -m first
"$real_git" -C "$product" remote add origin "$T/product-origin"
t_stub git "case \"\$*\" in
  *'push --dry-run origin HEAD:refs/heads/tehas-preflight')
    printf '%s\\n' \"\${GH_TOKEN:-}\" >\"\$T/push-token\"
    if [ -e \"\$T/push-refused\" ]; then cat \"\$T/push-refused\" >&2; exit 128; fi
    exit 0 ;;
esac
exec '$real_git' \"\$@\""
t_run "$install --check"
t_eq "$t_rc" 0
t_contains "$t_out" "Cycle token:  OK (tehas-bot[bot] can push to origin)"
t_eq "$(cat "$T/push-token")" ghs_cycle
printf '%s\n' "remote: Write access to repository not granted." \
  "fatal: unable to access 'https://github.com/acme/widget.git/': The requested URL returned error: 403" >"$T/push-refused"
t_run "$install --check"
t_eq "$t_rc" 1
t_contains "$t_out" "Cycle token:  CANNOT PUSH (tehas-bot[bot] was refused a dry-run push to origin)"
t_contains "$t_out" "    remote: Write access to repository not granted."
t_contains "$t_out" "The requested URL returned error: 403"
rm -rf "$product" "$T/push-refused" "$T/push-token" "$T/bin/git" "$th/state"/github*


t_case "a second install makes no labels and enables nothing"
rm -f "$T"/calls/*
t_run "$install"
t_eq "$t_rc" 0
# The user ids are asked again, and nothing else of GitHub.
t_eq "$(t_calls gh)" "api users/alice --jq .id"
t_eq "$(cat "$th/state/account-ids")" "alice $(t_uid alice)"
# Only the reset of a tehas unit systemd failed, which enables nothing.
t_eq "$(t_calls systemctl)" "--user is-failed tehas-dispatch.path
--user reset-failed tehas-*"

t_case "a host installed before needs no first install"
t_run "$install"
t_lacks "$t_out" "first install"

t_done
