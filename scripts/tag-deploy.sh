#!/usr/bin/env bash
# Deploy the newest release tag to a tehas host.
#
# The host updates with `tehas update`, which fast-forwards its platform
# checkout to the head of its branch and installs that. So this deploys the tag
# only when the tag is the head of the remote's default branch: it checks that
# first and refuses otherwise, checks that the ssh user has tehas, runs the
# update over ssh, then checks that the host's checkout is at the tag.
#
# Usage: scripts/tag-deploy.sh [--check] [--remote <name>] <ssh destination>
#   --check      run `tehas update --check` on the host: report, change nothing
#   --remote     the remote to read tags from, default origin
#   TEHAS_SSH    the ssh command, default "ssh". A host key or user is better
#                kept in ~/.ssh/config.
#
# Example: scripts/tag-deploy.sh tehas-tehas
#
# The work is `xtask tag-deploy` (console/xtask/src/deploy.rs), built with
# cargo from the workspace beside this script, or the build TEHAS_XTASK names.
# It still runs git and ssh for the steps above.
#
# Exit: 0 the host is at the tag, or --check found nothing to do; 1 the tag is
#       not the branch head, the host cannot be reached or the ssh user has no
#       tehas, the update failed or --check found work, or the host ended up
#       elsewhere; 64 a bad argument; another status when cargo cannot build
#       the task
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd) || exit 1
xtask=(cargo run --quiet --locked --manifest-path "$here/console/Cargo.toml" -p tehas-xtask --)
[ -z "${TEHAS_XTASK:-}" ] || xtask=("$TEHAS_XTASK")
exec "${xtask[@]}" tag-deploy --script "$here/scripts/tag-deploy.sh" "$@"
