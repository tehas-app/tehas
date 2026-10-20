# shellcheck shell=bash
# What a release tarball has to hold, so pack-release.sh and its test read one
# list rather than each keeping their own. Sourced, not run.
#
#   tehas_release_dirs      the directories of the checkout that install.sh
#                           copies from: its own bin/, libexec/ and lib/ and
#                           the host/ files it installs beside them
#   tehas_release_plain     the top-level files an adopter needs to install
#
# The console/ directory is in neither: the console comes from pack-release.sh's
# --console, and an adopter has no toolchain to build its source with. If
# install.sh reads a file outside these, a release packed without it is broken
# on the adopter's host, so both lists are checked against install.sh by
# test/pack_release_test.sh.

tehas_release_dirs() {
  printf '%s\n' bin libexec lib host
}

tehas_release_plain() {
  printf '%s\n' install.sh bootstrap.sh LICENSE NOTICE AGENTS.md
}
