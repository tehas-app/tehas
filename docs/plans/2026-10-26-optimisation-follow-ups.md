# Optimisation follow-ups: what was measured and left for later

Moved here from #760 on 2026-11-06. Optimisation ideas measured during #697
and left for later. Nothing here is planned; each item says when it becomes
worth doing. Add to this file rather than filing a new issue for each.

The sections below are as written on 2026-10-26. "Since then" at the end says
what has changed on main.

## Settings validation in Rust (was #755)

Decided 2026-10-26: not now.

- **What:** a `settings check` verb that holds the ~600 lines of checks in
  `lib/config.sh`. Bash still sources `instance.env`, scrubs it and exports
  what comes back (rule 3); a missing binary is 78.
- **Why not now:** since #756 a load starts only `nproc` and takes about
  14 ms. A Rust check adds a program start to every load, every test and
  every login shell, so it would not be faster.
- **Do it when:** a setting needs real parsing (a structured list, a URL
  taken apart), or the checks grow enough that `config_test.sh`'s
  shell-per-case cost matters.
- **First step:** `install.sh` loads settings before it builds the console,
  so that order changes first.

## Where the test suite's processes still go

Measured 2026-10-24 and 2026-10-26. The suite is bound by process starts:
system time is 2 to 3 times user time.

- **`dispatch_*` tests:** about 420 processes a case, mostly jq calls in
  `libexec/dispatch` and the gh stub. This goes when the dispatcher moves to
  Rust (#253, #254); nothing to do in the tests.
- **`cycle_record_test.sh`:** about 29,000 process starts (12,500 bash,
  4,500 jq, then head, tr, rm). `tehasbot cycle` already reads its progress
  once (#709). Next would be its jq calls, if the cycle supervisor stays in
  bash (#256).
- **`install_steps_test.sh`:** about 23,000 process starts, about six per
  file `install.sh` copies (cmp, stat, install, mv, sha256sum). That is the
  price of copying each file atomically, and it is left as is unless install
  gets a helper program.
- **Wall time over CPU:** `dispatch_start` ran 258 s wall against 111 s of
  CPU, and `clean` 171 s against 54 s before #703. Fixed sleeps and timing
  waits remain in some files; `t_hold`/`t_release` (#703) is the pattern for
  replacing them.

## The gate's time, for the record

- 2026-10-24: 3,452 s of CPU for the shell suite on the runner VM, and 11 to
  18 min in a cycle.
- 2026-10-26, a shared 16-CPU machine: main 4,541 s of CPU, #756 4,346 s.
- The two days are not one measure: the suite grew with v0.13.0, and the
  machines differ.

## Since then

Checked on main on 2026-11-06.

- The settings rules now live in the console (`console/core/src/settings/`),
  and `lib/config.sh` is 59 lines that load and ask the console. The first
  item is done another way.
- The dispatcher (#253, #254) and the cycle supervisor (#256) are in Rust,
  and `libexec/dispatch` and `libexec/cycle` are gone. The `dispatch_*` and
  `cycle_record_test.sh` counts above are from the bash versions; measure
  again before acting on them.
- `install_steps_test.sh` and the wall-time item stand as written.
