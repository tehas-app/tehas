# shellcheck shell=bash
# What a release tarball has to hold, so pack-release.sh and its test read one
# list rather than each keeping their own. Sourced, not run.
#
#   tehas_release_dirs      the directories of the checkout that install.sh
#                           copies from: its own bin/, libexec/ and lib/ and
#                           the host/ files it installs beside them
#   tehas_release_plain     the top-level files an adopter needs to install,
#                           and CHANGELOG.md, which says what an adopter does
#                           after updating to the release
#   tehas_release_extra     directories below the top that an adopter reads:
#                           the example instance, whose values tehas update
#                           names when a release asks for a setting
#
# The console/ directory is in neither: the console comes from pack-release.sh's
# --console, and an adopter has no toolchain to build its source with. If
# install.sh reads a file outside these, a release packed without it is broken
# on the adopter's host, so test/pack_release_test.sh reads the sources of
# install.sh's own file table and refuses one that no list of this file covers.

tehas_release_dirs() {
  printf '%s\n' bin libexec lib host
}

tehas_release_plain() {
  printf '%s\n' install.sh bootstrap.sh LICENSE NOTICE AGENTS.md CHANGELOG.md
}

tehas_release_extra() {
  printf '%s\n' instances/example
}
