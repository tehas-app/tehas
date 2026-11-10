#!/usr/bin/env bash
# The polyfills: what tehas does itself for a pi-rukas feature that no release
# has yet, and their removal once one has it.
#
# A polyfill of the feature <f>, a key of the features in the runner's
# --capabilities answer, is three things:
#   lib/polyfill-<f>.sh          the code, with two header lines naming where
#                                pi-rukas adds the feature, so a polyfill stands
#                                in for work upstream has taken on and never for
#                                one nobody will build:
#                                `# Upstream issue: <owner>/<repo>#<n>`
#                                `# Upstream PR: <owner>/<repo>#<n>`
#   test/polyfill_<f>_test.sh    its test
#   lines that use it            anywhere else, each a line of its own ending in
#                                `# polyfill <f>`
# A caller asks tehas-cycle caps mode, which answers native
# where the image has the feature, so a polyfill is not used once the image has
# it. Removing one is deleting those files and lines, which is what prune does.
# scripts/tag-release.sh runs prune before it tags.
#
# Usage: scripts/polyfills.sh [--root <dir>] list
#        scripts/polyfills.sh [--root <dir>] check
#        scripts/polyfills.sh [--root <dir>] expired <caps file>
#        scripts/polyfills.sh [--root <dir>] prune <caps file>
#   list     one line per polyfill: the feature, its upstream issue and its
#            upstream pull request
#   check    each polyfill has both upstream lines and its test, and each marked
#            line names a polyfill: one line per problem
#   expired  the polyfills whose feature the --capabilities answer in
#            <caps file> sets to true, one per line
#   prune    remove the expired ones from the tree, then check that each shell
#            file it changed still parses. Prints each path it removed or changed
#   --root   the tree, default the checkout this script is in
#
# It reads the tree and the answer and nothing else: no network, no git. That
# the issue and the pull request exist, and that the pull request was not closed
# unmerged, is checked by scripts/tag-release.sh, which asks GitHub.
#
# The work is `xtask polyfills` (console/xtask/src/polyfills.rs), built with
# cargo from the workspace beside this script, or the build TEHAS_XTASK names.
#
# Exit: 0 done, and for check no problem; 1 check found a problem, the answer
#       is not a JSON object, a file could not be changed, or a changed file no
#       longer parses; 64 a bad argument; another status when cargo cannot
#       build the task
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd) || exit 1
xtask=(cargo run --quiet --locked --manifest-path "$here/console/Cargo.toml" -p tehas-xtask --)
[ -z "${TEHAS_XTASK:-}" ] || xtask=("$TEHAS_XTASK")
exec "${xtask[@]}" polyfills --script "$here/scripts/polyfills.sh" "$@"
