#!/usr/bin/env bash
# Tests for install.sh: a build whose crate is missing from cargo's cache, a
# build that runs past its time and TEHAS_BUILD_TIMEOUT, a checkout without
# lib/paths.sh, and an analytics build that fails.
# shellcheck disable=SC2016  # stub bodies are expanded when the stub runs
# shellcheck source=test/lib.sh
. "$(dirname "$0")/lib.sh" || exit 1
# shellcheck source=test/install_lib.sh
. "$(dirname "$0")/install_lib.sh" || exit 1

# A crate that is not in cargo's cache: the offline build says so in cargo's own
# words, the retry without --offline builds, and the program is installed.
t_case "a build whose crate is missing from the cache is retried online once, and installs"
fresh
instance
checkout
rm -f "$T/calls/cargo"
t_env CARGO_OFFLINE=miss
# The note the retry prints goes to stdout while the host is still uninstalled,
# so this case pins the line itself. A full run prints its follow-ups as
# "next:", which update reads as steps: the note is not one of them.
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo | wc -l)" 14
t_contains "$(t_calls cargo | head -n 1)" "--offline"
t_lacks "$(t_calls cargo | tail -n 1)" "--offline"
t_contains "$t_out" "cargo's cache has no crate for"
t_eq "$(printf '%s\n' "$t_out" | sed -n '/^note: cargo.s cache has no crate/{s/^note: /STEP/;p}' | wc -l)" 7
t_eq "$(cat "$th/libexec/console")" "a console built from source"
t_eq "$(cat "$th/libexec/analytics")" "an analytics built from source"
t_run "$part/install.sh --check"
t_contains "$t_out" "Installed:    OK"
t_eq "$(t_calls cargo | wc -l)" 14
t_env

# The retry is reported from both attempts: the offline miss is not lost behind
# the online failure that followed it, and the note says the second build ran.
t_case "a cache miss that the fetch does not fix is reported from both attempts and installs nothing"
fresh
instance
checkout
rm -f "$T/calls/cargo"
t_env CARGO_OFFLINE=fail
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_eq "$(t_calls cargo | wc -l)" 2
t_contains "$(t_calls cargo)" "--offline"
t_contains "$t_out" "cargo's cache has no crate for"
t_contains "$t_out" "offline build:"
t_contains "$t_out" "online build:"
t_contains "$t_out" "could not compile"
t_eq "$(host_files | wc -l)" 0
t_env

# The two halves of the classifier: cargo's words for a missing crate send the
# retry, and words that merely quote --offline do not. A build script that
# echoes its own command line is the second kind.
t_case "words that only quote --offline are not a cache miss, so the build is not retried"
fresh
instance
checkout
rm -f "$T/calls/cargo"
t_env CARGO_OFFLINE=noise
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_eq "$(t_calls cargo | wc -l)" 1
t_contains "$(t_calls cargo)" "--offline"
t_lacks "$t_out" "building it again with the network"
t_eq "$(host_files | wc -l)" 0
t_env

# The other half of the rule: only a cache miss retries. The one package whose
# build fails fails for a reason the cache has nothing to do with, so that one
# call answers it and no second build is asked.
t_case "a build that fails for a reason other than the cache is not retried"
fresh
instance
checkout
t_env CARGO_FAIL=analytics
rm -f "$T/calls/cargo"
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_eq "$(t_calls cargo | wc -l)" 2
t_eq "$(t_calls cargo | grep -c -- --offline)" 2
t_contains "$t_out" "could not compile"
t_lacks "$t_out" "building it again with the network"
t_eq "$(host_files | wc -l)" 0
t_env

# The other wording for a missing crate, the one a locked build prints when the
# fetch itself is refused, sends the retry too. Deleting either of the words
# install.sh matches would leave one of these two cases red.
t_case "cargo's other words for a crate it could not fetch are a cache miss too"
fresh
instance
checkout
rm -f "$T/calls/cargo"
t_env CARGO_OFFLINE=fetch_miss
t_run "$part/install.sh"
t_eq "$t_rc" 0
t_eq "$(t_calls cargo | wc -l)" 14
t_contains "$(t_calls cargo)" "--offline"
t_contains "$t_out" "cargo's cache has no crate for"
t_eq "$(cat "$th/libexec/analytics")" "an analytics built from source"
t_env

# The retry can be cut off where the offline attempt was not: the miss is
# answered in cargo's own words, and the fetch that follows never ends. The
# message names the attempt that was cut off, which is the one without
# --offline, and reports both attempts.
t_case "the online retry that outlives the timeout is cut off and named as cut off"
fresh
instance
checkout
rm -f "$T/calls/cargo"
t_stub cargo 'offline=""
for a in "$@"; do [ "$a" != --offline ] || offline=1; done
if [ -n "$offline" ]; then
  echo "error: no matching package named \"analytics\" found" >&2
  exit 101
fi
sleep 20'
t_env TEHAS_BUILD_TIMEOUT=1
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "was cut off after 1s"
t_contains "$t_out" "offline build:"
t_contains "$t_out" "online build:"
t_lacks "$t_out" "--offline -p" # the offline argv, which was not the build cut off
t_eq "$(host_files | wc -l)" 0
t_env

# The same bound holds for the retry: a fetch that never answers is cut off
# rather than left to hang, and the message says so. The stub is the build, so
# a timeout in place of the real cargo is the whole case.
t_case "a build that outlives the timeout is cut off and named as cut off"
fresh
instance
checkout
if ! command -v timeout >/dev/null; then
  t_skip "no timeout on this host"
else
  rm -f "$T/calls/cargo"
  t_stub cargo 'sleep 3'
  t_env TEHAS_BUILD_TIMEOUT=1
  t_run "$part/install.sh"
  t_eq "$t_rc" 1
  t_contains "$t_out" "was cut off after 1s"
  t_eq "$(host_files | wc -l)" 0
  t_env
fi
t_stub_cargo
t_env

t_case "a TEHAS_BUILD_TIMEOUT from the environment that is not a positive whole number is refused before any build: 78"
for bad in 0 -5 1h "30;rm"; do
  fresh
  instance
  checkout
  rm -f "$T/calls/cargo"
  t_env "TEHAS_BUILD_TIMEOUT=$bad"
  t_run "$part/install.sh"
  t_eq "$t_rc" 78
  t_contains "$t_out" "TEHAS_BUILD_TIMEOUT is not valid: expected a positive whole number"
  t_eq "$(t_calls cargo)" ""
  t_env
done

t_case "a checkout without lib/paths.sh names the missing file"
fresh
instance
checkout
rm -f "$part/lib/paths.sh"
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_contains "$t_out" "$part/lib/paths.sh is missing from this checkout"
t_eq "$(host_files | wc -l)" 0

t_case "an analytics that does not build installs nothing, the console's build included"
fresh
instance
t_env CARGO_FAIL=analytics
t_run "$install"
t_eq "$t_rc" 1
t_contains "$t_out" "the analytics program did not build"
t_eq "$(host_files | wc -l)" 0
t_env

t_case "on an installed host, a failed analytics build leaves the new console uninstalled"
fresh
instance
checkout
t_run "$part/install.sh"
echo "// changed" >>"$part/console/core/src/mask.rs"
t_env CARGO_FAIL=analytics CONSOLE_BUILD="new source"
t_run "$part/install.sh"
t_eq "$t_rc" 1
t_eq "$(cat "$th/libexec/console")" "a console built from source"
t_eq "$(cat "$th/libexec/analytics")" "an analytics built from source"
t_env

t_done
