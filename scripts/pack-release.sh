#!/usr/bin/env bash
# Pack a release tarball for a tag, from the tree it is given and the arguments.
#
# An adopter downloads one tarball, unpacks it and runs install.sh from it, so
# what is packed is what install.sh reads: the directories, the top-level
# files and the example instance named in lib/release.list, the console
# binary it was given as libexec/console, a VERSION file with the tag, a COMMIT file with the commit
# it was packed from when one is given, and host/versions.env, to which
# the pins are put when they are given. The tarball holds one top directory,
# tehas-<tag>/, so unpacking it leaves that and nothing beside it. Its name
# names the architecture of the programs inside it, which the pack reads from
# their ELF headers rather than from what it was told to believe.
#
# The pins name what this release builds the sandbox from: PI_RUKAS_FOLLOW and
# PI_RUKAS_IMAGE_DIGEST, one line each at the end of versions.env. A release
# with no pins keeps pi-rukas's newest tag, as the checkout does today: passing
# none is allowed. Neither key has a reader yet: host/sandbox/build.sh derives
# the base image from the commit it follows, and the console once knew only
# release and branch:<name>, so a host that is given commit:<sha> stops on the
# setting until #463 teaches it that form and gives the digest a reader.
#
# The stage is a copy of the packed tree plus the console, in TMPDIR, taken
# down when the script ends.
#
# Usage: scripts/pack-release.sh <tag> <out dir> --console <path>
#                               [--arch x86_64|aarch64]
#                               [--analytics <path>] [--mcp <path>]
#                               [--store <path>] [--attach <path>]
#                               [--dispatch <path>] [--cycle <path>]
#                               [--commit <40 hex>]
#                               [--rukas-commit <40 hex>]
#                               [--rukas-digest sha256:<64 hex>]
#   <tag>        the release tag, in the form vX.Y.Z that tag-release.sh makes
#   <out dir>    where the tarball and its .sha256 go; it is created
#   --arch       the architecture the release is for, x86_64 (the default) or
#                aarch64: it names the tarball and says which e_machine every
#                program given has to have
#   --console    the built console binary, copied to libexec/console; a
#                workflow builds it and gives its path
#   --analytics  the built analytics program, copied to libexec/analytics
#   --mcp        the built MCP server, copied to libexec/tehas-mcp; the two are
#                the other programs install.sh puts in libexec, and a release
#                packed without them installs without them
#   --store      the built state store, copied to libexec/store. install.sh
#                makes and migrates the host's database with it, so a release
#                packed without it installs only on a host that has one
#   --attach     the built attachment checker, copied to libexec/attach. A
#                cycle given an issue's attachments reads their bytes with it,
#                so a host that has none fails a cycle that is given any
#   --dispatch   the built tehas-dispatch, copied to libexec/tehas-dispatch:
#                the poll; a host installed from a release packed without it
#                polls nothing, tehasbot dispatch stops with 78
#   --cycle      the built tehas-cycle, copied to libexec/tehas-cycle: the
#                cycle's start, supervision and report; a host installed from
#                a release packed without it starts no cycle, tehasbot cycle
#                stops with 78
#   --commit     the commit of tehas the tree is, 40 hex digits, written to
#                COMMIT. A tree with no git has no other way to say it, and
#                install.sh refuses a tree whose commit it cannot read
#   --rukas-commit   the pi-rukas commit this release's sandbox is built from,
#                    40 hex digits (upper case is accepted and written lower case)
#   --rukas-digest   the base image's digest, as sha256: and 64 hex digits, the
#                    same: upper case is accepted and written lower case
#
# The tree it packs is the working directory, or TEHAS_PACK_ROOT when that
# names one: the release workflow checks a tag out into its own workspace and
# points the script at it. It reads that tree and its arguments and nothing
# else: no network and no git. The work is `xtask pack-release`
# (console/xtask/src/pack.rs), built offline with cargo from the workspace
# beside this script, or the build TEHAS_XTASK names. It builds no console
# and writes nothing into the tree; what it writes is
# <out dir>/tehas-<tag>-linux-<arch>.tar.gz and its .sha256 beside it, each
# named only by its own name, so sha256sum -c checks them from that directory.
#
# Example:
#   scripts/pack-release.sh v0.7.6 dist --console console/target/release/console
#
# Exit: 0 the tarball and its .sha256 are written, their paths on two lines;
#       64 a bad argument (the tag is not vX.Y.Z, --arch names neither
#       architecture, --console is missing, is not a file or is the tree's own
#       build, --analytics, --mcp, --store, --attach, --dispatch or --cycle names no program, only one of the
#       two pins is given, the tree does not answer to TEHAS_PACK_ROOT, or an
#       out dir cannot be made or already holds that tarball); 1 the tree is
#       missing something install.sh reads, one of the programs given is no ELF
#       for the architecture named, a pin's or --commit's value has the wrong
#       shape, or staging, tar or the hash failed; another status when cargo
#       cannot build the task.
set -uo pipefail

here=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd) || exit 1
xtask=(cargo run --quiet --locked --offline --manifest-path "$here/console/Cargo.toml" -p tehas-xtask --)
[ -z "${TEHAS_XTASK:-}" ] || xtask=("$TEHAS_XTASK")
exec "${xtask[@]}" pack-release --script "$here/scripts/pack-release.sh" "$@"
