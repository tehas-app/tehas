---
title: Releases and Polyfills
description: "What a release is, how it is packed and verified, and what tehas does for a pi-rukas feature no release has."
---

## Releases

A release of tehas is one tarball an adopter can install without git and
without a Rust toolchain. GitHub publishes it for each `vX.Y.Z` tag; a person
gets it from the release's list of files.

`scripts/pack-release.sh` makes it, and the release workflow for a tag is a
thin wrapper around the same arguments:

```
TEHAS_PACK_ROOT='<checkout>' scripts/pack-release.sh <tag> <out dir> --console <path> \
  --analytics <path> --mcp <path> --store <path> --attach <path> --commit <40 hex> \
  --rukas-commit <40 hex> --rukas-digest <64 hex>
```

`--analytics`, `--mcp`, `--store` and `--attach` are the other four programs
`install.sh` puts in `libexec`, packed as `libexec/analytics`,
`libexec/tehas-mcp`, `libexec/store` and `libexec/attach`. Each is optional,
and a tarball packed without one has none.
`install.sh` makes and migrates the host's database with the store, so a
tarball without it installs only on a host that has one.

`--arch` names the architecture the release is for, `x86_64` (the default) or
`aarch64`, and nothing else is accepted: it is the middle of the tarball's name
and it says which e_machine every program given has to have, so a console built
for the other one is refused before anything is packed.

`--commit` is the commit of tehas the tree is, written to a `COMMIT` file. A
tree with no git has no other way to say it, the store records it in the
database, and `install.sh` refuses a tree whose commit it cannot read. It is
optional to the pack script, and the workflow always gives it.

The two pins are given together or not at all, so a release that does not pin
pi-rukas leaves both off. The digest is written as `sha256:` and 64 hex digits.

The tree it packs is the working directory, or `TEHAS_PACK_ROOT` when that names
one, so a workflow that checks a tag out into its own workspace points the
script at the checkout rather than at the runner's copy of the script. It reads
that tree and its arguments and nothing else: no network, no git. The work is
`xtask pack-release` in the workspace's `console/xtask`
(`console/xtask/src/pack.rs`), which the script builds offline with cargo, or
the build `TEHAS_XTASK` names; the release runner holds cargo already. It
builds no console of its own; the workflow compiles it for linux x86_64 and
hands the binary over. Which
architecture the binaries are is read from the programs themselves, from the
two bytes of e_machine at offset 18 of each ELF header, so a release is not
packed on what whoever ran the workflow believed.

Usage, the list of what is packed and the exit statuses are the script's: run
`scripts/pack-release.sh --help`. Three of them matter to the workflow that calls
this one: half a pin is a bad argument (64), a pin whose value has the wrong
shape is a failure (1), and the two are gated differently; so is a program built
for the other architecture, a failure too (1), since the argument itself is
never wrong. A pin of either shape
is written to `versions.env` in lower case, so a value `git rev-parse` or a
registry gave in upper case is accepted.

What it writes is `tehas-<tag>-linux-<arch>.tar.gz` and one `.sha256` file
beside it, and it writes neither before both are ready, so a run that fails
leaves no tarball for the next one to be refused against. Unpacked, the tarball
is a single `tehas-<tag>/` directory holding what `install.sh` reads: its
`bin/`, `libexec/`, `lib/` and `host/`, the top-level files an install needs,
`instances/example/`, the console as `libexec/console` and the other three
programs when they were given, a `VERSION` file with the tag, a `COMMIT` file
with the commit when it was given, `CHANGELOG.md`,
and `host/versions.env`. `install.sh` run from such a tree has no `console/` to
build from: it installs the programs the release carries as they are, with
`release <sha256>` as the stamp, asks cargo nothing, and keeps the installed
copy of a program the release does not carry. The list is `lib/release.list`, the one place it is written
down, and the gate checks the script and the test against it. An adopter
unpacks it and runs `./install.sh` from inside, as they would from a checkout.
`console/`, the instance directories and the git record are not packed: there
is nothing in the tarball to build from and nothing to check out, and
[The host](/host#configuration-comes-from-git) tells how a checkout installs the
console by building it instead. The `VERSION` file is what makes that install
say which release it is: the version `tehas api` and the footer of every page
read is the `version.txt` that `install.sh` writes beside the pages, and a tree
with no git has no commit to write, so it writes the tag from `VERSION`.

A tree with a symbolic link under any packed directory is refused: a link is
kept by the copy and followed by `tar`, so its target's content would reach the
release from outside the tree. That is why the tree is a checkout of this
repository and the command makes no other claim about where it came from.

The two pins, the last two lines of the copy of `versions.env` inside the
tarball, replace rather than repeat a key the tree already carried:

```
PI_RUKAS_FOLLOW=commit:<sha>         the pi-rukas commit the sandbox builds from
PI_RUKAS_IMAGE_DIGEST=sha256:<hex>   upstream's image of that commit, no longer the base
```

The rest of the file is the checkout's own. The release workflow packs no pins:
a release follows the pi-rukas line its `versions.env` names, as a checkout
does, so a new pi-rukas reaches a host at its next `tehas update`. The pins are
for a person who packs a release that must keep one pi-rukas.

### Verifying a download

The `.sha256` beside the tarball is in the format `sha256sum -c` reads, and
names the tarball alone, so the check runs in the directory the two files are
in:

```
sha256sum -c tehas-v0.7.6-linux-x86_64.tar.gz.sha256
```

The public key for a signature is `host/release/minisign.pub`, and its
fingerprint is `FE1F47199BDF1A8A`, named in the key's own first line. It is not
the tarball that holds the copy to check with: whoever packs the release signs
it, and a key that arrives inside the download it authenticates is the
attacker's, not the project's. The key is read from the repository, or from
`~/tehas/release/minisign.pub`, where `install.sh` puts it, and compared with
that fingerprint before `minisign` is asked anything:

```
minisign -Vm tehas-v0.7.6-linux-x86_64.tar.gz -p ~/tehas/release/minisign.pub
```

The release workflow signs the tarball and the installer, and checks each
signature with the key in the tagged tree before it uploads anything. The
`sha256sum -c` above proves only that the file is the one published beside that
hash: a download and its hash arrive together, so a check that passes on a
tampered pair says nothing about where they came from. The signature is the
check that does.

### Release notes and the Adopter action

`CHANGELOG.md` at the top of the repository has one section per release,
newest first: `## vX.Y.Z (YYYY-MM-DD)`, then `### Changes` and
`### Adopter action`, the steps an adopter takes after updating: settings added
or renamed, root steps, anything by hand. "None." is a valid action. Work in
progress goes under `## Unreleased`, and a person renames it to the version in
the release's pull request.

`scripts/tag-release.sh` reads `CHANGELOG.md` from the commit it is about to
tag, `--dry-run` too, and refuses with exit 1 when the section for the new tag,
or else `Unreleased`, has no `### Adopter action` heading with a line under it.
It edits the file only to collect the fragments and to name the moves of the
pinned tools, below.

`scripts/release-notes.sh <tag>` prints a section without its heading, and
exits 1 when the file has none for that tag. The workflow makes the release's
page from it, from the `Unreleased` section when the tag has none of its own.

A pull request does not edit `CHANGELOG.md`, so two open ones do not conflict
there. It adds a fragment, `changelog.d/<issue>.md`, with a `### Changes` and
an `### Adopter action` section, either of which may be left out; text outside
them is refused. A pull request with no issue names the file after its branch,
each `/` a `-`: branch `fix/typo` writes `changelog.d/fix-typo.md`, and
`scripts/collect-changelog.sh --name <branch>` prints that path. A line inside
a fenced code block is text, so a `#` comment there is not read as a heading;
a line in one that starts `## ` or `### ` is refused, since it would read as a
heading of `CHANGELOG.md` once collected, and so is a block left open.
`scripts/release-notes.sh Unreleased` prints the Unreleased section with the
fragments' lines added under their headings, ordered by issue number. A tagged
release is read from `CHANGELOG.md` alone.

`scripts/tag-release.sh` counts the fragments' Adopter action, and `--dry-run`
prints the section with the fragments collected. When the head has fragments,
the head's own `scripts/collect-changelog.sh` (the version in the commit being
tagged, not the operator's checkout) moves their lines into the release's
section and deletes them; then it runs the gate, opens a pull request from
`changelog-<tag>`, and tags nothing; the tag, and the move of the pinned tools,
wait for a person to merge it. An open pull request from such a branch holds
the tag. A head with fragments and no `scripts/collect-changelog.sh` is not
tagged. `scripts/collect-changelog.sh --print` shows the result without
changing a file. The rules are `xtask collect-changelog`
(`console/xtask/src/changelog.rs`), which the script runs.

A host moving from its platform checkout has no version installed, so its notes
start at the newest `vX.Y.Z` tag at or before the checkout's HEAD, which
`tehas update` reads with `git describe` from what the repository already has;
with no such tag it is shown every release.

### Pinned tools move with the release

Every tool in `host/versions.env` and in the npm trees beside it
(`host/npm/`, `host/sandbox/npm/`) is pinned to an exact version, and
`bootstrap.sh` installs only what the file says. `scripts/tag-release.sh`
moves the pins before it tags, through `scripts/pins.sh move` (`console pins`):
each to the newest release of the major version it has, taken from the tool's
own source: nodejs.org, GitHub releases (gh, bun, shellcheck, jq, Rust,
codebase-memory-mcp) or tags (rustup), npm, crates.io (vipune, oo) and Docker
Hub (Caddy). Each version moves with what the header of `versions.env` pairs
with it: the publisher's checksum or the hash of the download, the lockfile
of an npm tree (npm locks it again; a version younger than npm's
`min-release-age` is not taken) and its `_LOCK_SHA256`, Caddy's digest in the
three files that name it, and vipune's model at the revision the new vipune
asks for, with its two hashes from Hugging Face. pi-rukas is not pinned and is
left alone.

Under 0.x a minor release can break, so "same major" moves vipune from 0.15 to
0.20. A later major version is never taken. The release's section of
`CHANGELOG.md` names each move under `### Changes` and each major held back
under `### Held back`, for a person to decide.

When anything moved, the script runs the gate there, pushes a branch
`pins-<tag>`, opens a pull request and exits 2 without tagging: verify.yml runs
on that commit, a person merges it, and the next run tags. While such a pull
request is open the script names it and exits 2. A source that cannot be read,
or a move that fails, stops the release with 1, before any factory installs the
tool. `--no-pins` tags without moving them. `scripts/pins.sh check` prints each
pin and what it would move to, and changes nothing. A factory takes the new
tools when its tag moves, on its next `bootstrap.sh user`.

### The release workflow

`.github/workflows/release.yml` runs on the push of a `vX.Y.Z` tag, on the
organisation's own runner as the gate does. It builds the console, the
analytics program, the store and the MCP server (static, musl) with `cargo
build --release --locked`, resolves the pins, runs
the pack command above (which refuses any program whose e_machine is not the
one its `--arch` promises), signs the tarball and `host/release/install` (when the
tag has one) with minisign, checks both signatures against
`host/release/minisign.pub`, and publishes the release with the tarball named
above, its `.sha256` and `.minisig` beside it, and `install` with
`install.minisig`.

The release is made as a draft and published only when every file is on it.
The signing key is the Actions secret `MINISIGN_SECRET_KEY`, the whole secret
key file, and its password, if it has one, is `MINISIGN_PASSWORD`. A run that
cannot sign, or whose signature the key in the tree does not check, publishes
nothing.

### The installer

`host/release/install` is the first install of a host, a POSIX shell script
run as root. A release publishes it as the asset `install`, beside the tarball,
its `.sha256` and its `.minisig`, which are the three names `tehas update`
downloads:

```
curl -fsSL https://github.com/tehas-app/tehas/releases/latest/download/install | sudo sh -s -- boss
```

It downloads the newest release (or `--version vX.Y.Z`) of this machine: the
asset is `tehas-<version>-linux-<arch>.tar.gz`, `<arch>` the architecture
`uname -m` answers, `x86_64` or `aarch64`, with `amd64` and `arm64` read as
those. A machine whose `uname -m` says anything else stops before anything is
downloaded, with the words it answered. It checks the `.sha256`
and the signature with the key written into the script, which
`test/release_install_test.sh` keeps equal to `host/release/minisign.pub`, and
unpacks it. It installs `minisign` with apt-get when the host has none. It runs
that release's `bootstrap.sh root <user>` (not with `--no-bootstrap`), then
puts the release in `~<user>/tehas/versions/<version>`, points
`~<user>/tehas/current` at it, gives both to the user and says to run
`tehas setup` as that user. A failed check or a failed `bootstrap.sh` puts
nothing in the user's home. The script cannot check itself: to check it before
running it, download `install` and `install.minisig` and run
`minisign -Vm install -p host/release/minisign.pub` with the key from the
repository. While the repository is private the download needs
a token: download the three files with `gh release download <tag> --pattern
'tehas-*'` and pass their directory with `--from <dir> --version <tag>`.

### What the gate covers, and what a person adds

The pack script is in the fast gate, with `test/pack_release_test.sh`: it packs
from the test's own tree with a stub console and checks the file list, `VERSION`,
the pin lines, that the `.sha256` passes `sha256sum -c`, what a bad argument
costs, that a tree holding a symbolic link is refused, that the sources
`install.sh` reads are covered by `lib/release.list`, and that the command line
above is a command that works. `test/tag_release_test.sh` checks the Adopter
action rule and `test/pack_release_test.sh` the release notes. The release
workflow is not run by the gate and is not written by a work cycle: CI files are
protected, so a person changes it against the interface above.

## Polyfills of pi-rukas features

A polyfill is what tehas does itself for a pi-rukas feature that no release has
yet, so tehas need not wait for the release. It ends when pi-rukas has the
feature. It is only for work pi-rukas has taken on: an issue that asks for the
feature and a pull request that builds it. A feature with no pull request
upstream gets no polyfill; it is asked for upstream first. Each check of a pi-rukas feature asks one function,
`tehas-cycle caps mode <feature>`, given the answer on standard input, where `<feature>`
is a key of `features` in the runner's `--capabilities` answer. It says one of
four things:

| Answer     | When                                                    |
| ---------- | ------------------------------------------------------- |
| `native`   | the answer sets the feature to `true`                    |
| `polyfill` | it does not, and tehas carries `lib/polyfill-<feature>.sh` |
| `off`      | neither                                                 |
| `error`    | the answer could not be read: not a JSON object, or a `features` that is not an object. Status 1 |

`native` wins, so a host stops using a polyfill the moment its sandbox image
has the feature, before the code is removed. The refresh (`continuePr`) and the
planning turns (`planTurns`) are read this way. Neither has a polyfill.

An answer that cannot be read is `error`, never `polyfill`, so a probe that
failed cannot read as a feature tehas should supply. An object with no
`features` key is the answer of an image that has none, and is answered
`polyfill` or `off`.

A polyfill of the feature `<f>` is three things, and nothing else may use it:

- `lib/polyfill-<f>.sh`, with two header lines naming where pi-rukas adds the
  feature: `# Upstream issue: <owner>/<repo>#<n>` and
  `# Upstream PR: <owner>/<repo>#<n>`
- `test/polyfill_<f>_test.sh`, its test
- the lines that call it, each a line of its own ending in `# polyfill <f>`,
  such as the `polyfill)` arm of a `case` on the mode

A polyfill gives the container nothing a cycle does not already get: the
mounts and variables of `tehas work` stay as `test/work_test.sh` pins them.
What cannot be done that way is not a polyfill; it waits for pi-rukas.

`install.sh` installs every `lib/polyfill-*.sh` of the tree it runs from into
`~/tehas/lib/`, beside the libraries it names one by one, so a host answers the
way the checkout does. A tree that carries none installs none.

`scripts/polyfills.sh` reads a tree and a `--capabilities` answer and nothing
else:

```
scripts/polyfills.sh list               # each polyfill, its upstream issue and pull request
scripts/polyfills.sh check              # each names both and has its test, each marker a polyfill
scripts/polyfills.sh expired caps.json  # the ones the answer makes unnecessary
scripts/polyfills.sh prune caps.json    # remove those: their files and marked lines
```

`prune` checks that each shell file it changed still parses, and exits 1 when
one does not: a marked line that was part of a larger statement. The fast gate
runs `check` on this checkout.

### Removed before a release

`scripts/tag-release.sh` does this when the head carries any polyfill. It asks
the pi-rukas that `host/versions.env` follows for its `--capabilities`, in the
image upstream publishes for that commit, or reads the answer from
`--rukas-caps <file>`. When the answer has the feature of a polyfill, it
removes that polyfill on a branch `remove-polyfills-<tag>`, runs the gate there,
pushes the branch, opens a pull request and exits 2 without tagging. A person
merges it and runs the script again. While a pull request from a
`remove-polyfills-` branch is open, the script names it and exits 2 again.
Nothing merges on its own.

A pi-rukas that cannot be asked stops the release with 1: whether a polyfill
is still needed is not known.

Each polyfill it keeps is checked against GitHub. When its upstream issue or
pull request cannot be found, or the pull request was closed without a merge,
the release stops with 1 and names it: nothing upstream will replace it, so it
is no longer a polyfill. Name a live pull request in it, or remove it. A
polyfill whose pull request is merged while the answer still lacks the feature
is named and kept: the feature is not released yet, or came under another
name, and a person decides. `check` reads only the tree, so the gate checks
that both lines are there, not that GitHub has them.
