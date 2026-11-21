# The commands of other parts: `tehas <part>`

**Status**: implemented on the branch `parts-front`, 2026-11-21. For
tehas 1.x, as an addition.

## Why

The tehas family has more than one command: `tehas` in a factory,
`industry` on the machine that runs several factories, and later Kaizen,
Yokoten and the others. A person should need one name. `tehas` is the one
they know, so every part's command is reachable under it:

```
tehas industry factory list      # runs: tehas-industry factory list
tehas kaizen ...                 # runs: tehas-kaizen ...
```

tehas stays a single-project appliance. It names no part's commands and
reads none of their settings. It only finds a program and runs it, as git
runs `git-<word>` and cargo `cargo-<word>`.

## The rule

One rule, in `console/core/src/parts.rs`, kept by both programs below.

1. **A word of tehas wins.** A part is looked for only when the first word
   is no command of `tehas`. Every 1.x command line that works today works
   the same.
2. **A part is `tehas-<word>` on `PATH`.** The first executable file of
   that name in an absolute directory of `PATH`. An empty or relative
   directory of `PATH` is not searched.
3. **A part's word is a name.** A lowercase letter, then lowercase letters
   and digits, in groups joined by one hyphen, at most 32 bytes. Anything
   else (`../x`, `--json`, `7`) is no part.
4. **tehas's own programs are no part.** The directories `~/tehas/libexec`
   and `~/tehas/bin` are not searched, and the words of `parts::OWN`
   (`contractor`, `cycle`, `dispatch`, `front`, `mcp`, `proxy`, the names
   of tehas's own `tehas-*` programs) are never looked for.
5. **The family's words are reserved.** `parts::FAMILY` is `industry`,
   `kaizen`, `kata`, `monozukuri` and `yokoten`. tehas never takes one for a
   command of its own, which a test of `console/src/front.rs` holds. The
   list is only added to within 1.x.
6. **The part runs in place of tehas.** By exec, with the remaining words as
   they were given and the environment as it is. Its output and its exit
   status are its own. tehas loads no settings and takes no lock first.
7. **Help goes to the part.** `tehas help <part>` runs `tehas-<part> --help`,
   and `tehas <part> --help` passes `--help` on as any other word.
8. **No part, no change.** A word that is no command and no part is exit 64
   with the usage on standard error, as before. A part that is found but
   cannot be run is exit 1.
9. **`tehasbot` runs no part.** It is the units' command.

`tehas` and `tehas help` end with the parts found on `PATH`, when there are
any: `Parts on this machine: industry, kaizen`.

## Two programs

**A factory's tehas.** `~/tehas/bin/tehas` runs `console front`
(`console/src/front.rs`), which applies the rule when its own lookup finds
no command.

**The front, for a machine with no factory.** The machine that runs
industry has no tehas: each factory has its own machine. `console/front`
builds `tehas-front`, a small static program that holds only the usage and
the rule. Every release carries it as its own signed asset:

```
tehas-front-vX.Y.Z-linux-x86_64            the program
tehas-front-vX.Y.Z-linux-x86_64.sha256     its hash
tehas-front-vX.Y.Z-linux-x86_64.minisig    its signature, by the release key
```

A part's install puts it in place as `/usr/local/bin/tehas` on a machine
with no factory, after checking the signature with
`host/release/minisign.pub`. On that machine `tehas status` is exit 64 with
the usage, which says this machine runs no factory.

The front searches every absolute directory of `PATH`: it has no libexec of
its own.

## How a part joins

A part needs no change in tehas to be reachable:

1. Install a program `tehas-<part>` on `PATH`: a link to the part's own
   command is enough. It receives the words after `tehas <part>`.
2. On a machine with no factory, install the front of a tehas release as
   `/usr/local/bin/tehas`, unless a `tehas` is there already.
3. To reserve the word for good, so that no later tehas takes it, add it to
   `parts::FAMILY` in a tehas pull request. Industry and Kaizen are reserved
   from the start.

Industry does 1 and 2 from industry 1.0: its release carries
`tehas-industry`, and its host install puts the front in place.

## The contract

Within 1.x the rule above is public: the lookup, the names, the reserved
words, the exit statuses of the front, and the front asset's name. It is an
addition only. A part's own commands, flags and statuses are that part's
contract, not tehas's.

A later 1.x may add a command of its own whose word a part on someone's
`PATH` has, unless the word is reserved: the command wins, by rule 1. That
is why a part reserves its word.

## Old commands in 1.x, and 2.0

Every command that worked before keeps working through 1.x, v1.2.0 included:

- each command of a factory's `tehas`, as rule 1 holds;
- each part's own command, typed by its own name: `industry <words>` on the
  host is `industry` as before, beside `tehas industry <words>`;
- an unknown word, still exit 64.

Nothing in 1.x calls the separate commands old or warns when one is typed.

2.0 may retire them: `tehas <part>` becomes the one way in, and a part's
own name (`industry`, and each later one) is taken out of the code and the
docs. Before that a part moves what calls its old name to `tehas-<part>`
or `tehas <part>`: for industry, its systemd units, its installer and the
commands its docs show.

## Left out

- The login wrapper `host/login/tehas`, which other accounts of a factory
  host run, runs no part. No part is installed on a factory's machine today.
- The front is built for x86_64, as the release is. An aarch64 build comes
  with an aarch64 release.
- Completion of a part's words. The part may ship its own.

## Tests

- `console/core/src/parts.rs`: the name rule, the lookup and the list.
- `console/src/front.rs`: no command takes a family word.
- `console/tests/parts.rs`: the factory's tehas runs a part with its words
  and status, sends help to it, refuses its own programs and directories,
  lets a command win, and lists the parts in the usage.
- `console/front/tests/front.rs`: the front does the same with no factory.
