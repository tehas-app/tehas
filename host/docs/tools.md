---
title: Commands
description: "Every tehas command and what it does."
---

# Commands

On the host as the tehas user (in the example, `boss` on `tehas-tehas`).
Interactive shells get the environment from `~/tehas/env.sh`; a non-interactive
`ssh boss@tehas-tehas '<command>'` must source it first.

There are two commands on `PATH`, both in `~/tehas/bin`:

```
tehas dispatch [--dry-run]       # one poll of the issue queue, by hand
tehas work <issue> [--restart] [--driver lean|classic]   # one cycle for an issue, by hand
tehas plan <issue> ...           # one turn of a planning session, by hand
tehas dev [<name>]               # an interactive session on the host, by hand
tehas index                      # build the code index the agents search
tehas setup [options]            # write this host's instance and start the factory
tehas update [--check]           # bring this host up to date with the platform
tehas labels                     # create the tehas labels in the repository
tehas panel [set ...|clear ...]  # the pages of the project the dashboard links
tehas host-message [set ...|clear ...]  # short messages for this factory
tehas check-product [--sandbox]  # check the product against the contract
tehas init-product [--dry-run]   # propose the missing contract files
tehas status [--json]            # print the state of the factory
tehas api [--json]               # what this factory speaks, for a program
tehas disk [--full] [--json]     # how full this host is, and what fills it
tehas limit [set ...|clear]      # an outside cap on what this factory starts
tehas order [set ...|clear]      # the order ready issues start in
tehas start <issue> [--clear]    # approve an issue for a start, from the host
tehas drain [--wait S] [--json]  # start nothing new, wait for running cycles
tehas freeze <issue> [--reason R]  # hold a running cycle still, its clocks stopped
tehas thaw <issue>               # let a frozen cycle go on
tehas history [--json] [--after C] [--issue N]  # what ended, oldest first
tehas export [--json] --after C <dir>  # copy a cycle's records, masked
tehas state pack|unpack -        # move a stopped factory's state to another machine
tehas state check [--json]       # the state store's integrity and rows
tehas state copy --to <file>     # a copy of the store, taken while cycles run
tehas state load --from <file>   # fill a drained factory's empty store from one
tehas top [--once|--json]        # one screen of what the factory is doing
tehas analytics report|export    # the factory's last 365 days of runs
tehas mcp key                    # a new key for the read-only MCP server
tehas pause [reason]             # stop this host from starting new work
tehas resume                     # lift this host's pause flag
tehas stop <issue> [--keep-workspace]  # end a cycle by hand, clear what it left
tehas help [command]             # this list, or the text of one command
```

`tehas` is the one a person types. [`tehasbot`](/tools-tehasbot#tehasbot) is what the timers
and services run. Each of their commands is a verb of the console in
`~/tehas/libexec`, or a program beside it: `install.sh` installs three bash
scripts from `libexec/` in the platform repository (`dev-login`, `setup` and
`update`: `tehas setup` and `tehas update` run the last two, and the sudo rule
of `bootstrap.sh dev` runs the first) and the Rust programs it builds (the
console, analytics, the MCP server, the state store, the attachment checker,
`tehas-dispatch` and `tehas-cycle`). The two commands only choose one and run
it, and exit with its status. `tehas status`, `tehas disk`, `tehas top`,
`tehas dev` and `tehas mcp key` need no settings, and `tehasbot mcp key` and
`tehasbot github-writes` are verbs too.
The verbs
that do, `tehas state`, `limit`, `pause`, `analytics`, `stop`, `init-product` and
`tehasbot health`, `clean`, `preview`, `protolab`, `refresh` among them, run
under `console run`, which checks the arguments, loads the settings, requires
what the command cannot run without and runs it (`console/src/run.rs`), and so
do `tehas dispatch`, `plan`, `check-product` and `tehasbot dispatch`, `review`,
`improve` and `github-token`, which have no script of their own. A command neither of them knows
is exit 64 with the list on standard error. `tehas <command> --help`
prints the same text as `tehas help <command>`.

Every command that needs the instance settings loads them first and stops with
exit 78 and one line if a setting it needs is missing or malformed. See
[The host](/host).

The two lists name every command the two programs route. Four of them have no
section of these pages and are on the page that describes them: `tehas panel`
and `tehas host-message` on [the factory API](/api#tehas-panel) and
[the factory API](/api#tehas-host-message), `tehas mcp key` and
`tehasbot mcp serve` on [The MCP server](/mcp-server#how-it-runs), which
`tehas-mcp.service` runs. `tehas state copy --to <file>` and
`tehas state load --from <file>` are the two verbs `tehas state` holds beside
[`pack`, `unpack` and `check`](/tools-control#tehas-state), on
[the factory API](/api#tehas-state-copy).

## Instance settings

Set in `instance.env` in the instance directory. `instances/example/instance.env`
in the platform repository has every one with a comment.

| Setting                         | Default                                  | Meaning                                                         |
| ------------------------------- | ---------------------------------------- | --------------------------------------------------------------- |
| `TEHAS_REPO`                    | required                                 | `owner/name` of the repository whose issues are the queue       |
| `TEHAS_CONTROL_ISSUE`           | required                                 | Issue that carries `tehas:pause`                                |
| `TEHAS_ALLOWED_LABELERS`        | required                                 | Logins whose labels count, space separated: `tehas:ready`, `tehas:plan`, `tehas:refine`, `tehas:finish`, `tehas:blocked` and `tehas:lean` for the dispatcher, `tehas:prototype` for the Protolab. Each counts only with the GitHub user id `install.sh` and `tehas update` recorded for it, see [Accounts by user id](/tools-update#accounts-by-user-id) |
| `PI_ENSEMBLE_SUBAGENT_MODEL`    | required                                 | Model for every subagent role without one of its own            |
| `PI_ENSEMBLE_SUBAGENT_PROVIDER` | required                                 | Its provider, as named in `models.json`                         |
| `PI_ENSEMBLE_MODEL_<ROLE>`      | `PI_ENSEMBLE_SUBAGENT_MODEL`             | Model for one role: `EXPLORE`, `DEVELOPER`, `ADVERSARIAL_DEVELOPER`, `CODE_REVIEW_SPECIALIST` (the six lens reviewers) or `OPS`. Another name is refused. See [A model per role](/host#a-model-per-role) |
| `PI_ENSEMBLE_PROVIDER_<ROLE>`   | `PI_ENSEMBLE_SUBAGENT_PROVIDER`          | Its provider. Refused without the role's model                  |
| `TEHAS_HOST_ADDR`               | `127.0.0.1`                              | Address the docs server and the edge router bind to             |
| `TEHAS_INDEXING`                | `0`                                      | `1` lets search engines index the dashboard and the manual      |
| `TEHAS_ANALYTICS_SRC`, `_SITE`  | empty                                    | Optional. The https address of a visitor analytics script and the site name it reports under. Both set: every page loads it |
| `TEHAS_TOP`                     | `0`                                      | `1` lets [`tehasbot top`](/tools-health#tehasbot-top) publish the host snapshot to `~/tehas/live/top.json`, which the Host panel of the factory floor and the plain dashboard read. With `0` the command removes the file and writes nothing. It governs the published file only: `tehas top` in a terminal always works, and the unit, installed on every instance, starts only when you enable it |
| `TEHAS_ANALYTICS_COOKIE`        | `0`                                      | `1` adds `data-cookie="true"` to that script tag                |
| `TEHAS_ENERGY_SOURCE_URL`       | empty                                    | Optional. The address the `DATA SOURCE: <name>` line at the end of the energy page links. Empty: the page shows the name as plain text. An `https://` address, else every command stops with 78 (see [`tehasbot energy`](/tools-health#tehasbot-energy)) |
| `TEHAS_DESIGN`                  | `beesknees26`                            | The look of the whole site and its front page: `beesknees26` (the business page at `/`) or `pixel` (the factory floor at `/`). Any other value is exit 78. See [Designs](/host#designs) |
| `TEHAS_CYCLE_REPORT`            | `rust`                                   | Who reports a cycle: `rust`, `tehas-cycle cycle`, the only value. The report in bash is gone: `bash`, or any other value, is exit 78. See [The report in Rust](/operating#the-report-in-rust) |
| `TEHAS_ANALYTICS`               | `on`                                     | The factory's own analytics, not the visitor analytics above: 365 days of runs, issue events and poll samples in `~/tehas/state/analytics.db`. `off` stops every write and leaves the file as it is. Anything but `on` or `off` stops every command with 78 |
| `TEHAS_PROTOLAB_URL`            | `https://lab.<domain>`, or port 8081     | Address people reach the Protolab at, when it is published elsewhere, as `https://lab.example`. The dashboard links to it. Without it: `https://lab.<domain>` when `TEHAS_DOMAIN` is set, else port 8081 of the dashboard's host |
| `TEHAS_DOMAIN`                  | none                                     | The domain the edge's names are made under, as `tehas.app`: `uat-<pr>.` for a preview, `lab-<issue>.` for a ticket of the Protolab, `lab.` for its list |
| `TEHAS_PREVIEWS`                | `0`                                      | `1` builds a preview of every open pull request whose branch is in the product repository and that an owner, member or collaborator opened. Needs `TEHAS_DOMAIN` |
| `TEHAS_EGRESS`                  | `0`                                      | `1` puts every cycle, planning turn, review and `tehas dev` session on an internal network whose one way out is a proxy that connects only to listed hosts. See [Where a container may connect](/host#where-a-container-may-connect) |
| `TEHAS_EGRESS_HOSTS`            | empty                                    | Hosts the proxy allows beside its own list, one space apart, each a name on port 443 or `name:port`, as `pypi.org files.pythonhosted.org` |
| `TEHAS_PREVIEW_MAX`             | `5`                                      | How many previews run at once. The newest pull requests win |
| `TEHAS_PREVIEW_MIN_FREE`        | `5g`                                     | Free disk space below which no preview is built |
| `TEHAS_CYCLE_MIN_FREE`          | `10g`                                    | Free disk space below which no cycle starts: the smaller of the file system of `~/tehas` and the one Docker keeps its images on, read with `df`. Space that cannot be read holds too. Running cycles are not touched (see [Limits that hold work back](/operating#limits-that-hold-work-back)). A size such as `512m` or `10g`, else every command stops with 78 |
| `TEHAS_IMPROVE_REPO`            | empty                                    | Optional. `owner/name` of the repository the [improvement loop](/tools-protolab#tehasbot-improve) files its findings in. Empty: no loop. Never `TEHAS_REPO`: the findings are a model's, and a person triages them into tickets of the product first, so loading refuses it with exit 78 |
| `TEHAS_IMPROVE_RUKAS_REPO`      | empty                                    | Optional. `owner/name` of the repository the loop files in what only a change in pi-rukas can fix. Empty: those findings go to `TEHAS_IMPROVE_REPO` with the rest. Needs `TEHAS_IMPROVE_REPO`. Never `TEHAS_REPO` |
| `TEHAS_IMPROVE_RUKAS_SKIP`      | `0`                                      | `1`: the loop files no finding about pi-rukas anywhere. Not with `TEHAS_IMPROVE_RUKAS_REPO` |
| `TEHAS_IMPROVE_MAX_OPEN`        | `10`                                     | How many issues labelled `tehas:improvement` may be open in one repository before the loop files no more there |
| `TEHAS_REVIEW_REPO`             | empty                                    | Optional. `owner/name` of the repository [tehasbot review](/tools-plan#tehasbot-review) files the findings of a run in, as one issue a person triages. Empty: a run that found something says it filed nothing. Never `TEHAS_REPO`: the findings are a model's, and a person triages them into tickets of the product first, so loading refuses it with exit 78 |
| `TEHAS_REVIEW`                  | `0`                                      | `1` lets [tehasbot review](/tools-plan#tehasbot-review) review anything: `tehas-review.timer` starts it once a week and a run that is off exits 0 at once and calls nothing. Anything but `0` or `1` stops every command with 78 (see [the review loop](/operating#the-review-loop)) |
| `TEHAS_MILLWRIGHT_LOGIN`        | the app's login                          | Not a setting. The millwright is the bot, the login in `~/tehas/state/github/login`, or the app `TEHAS_MILLWRIGHT_APP` names, whose `tehas:ready`, and whose answer to a planning question, the dispatcher takes on [the millwright's grant](/operating#the-millwrights-grant). Never one of `TEHAS_ALLOWED_LABELERS`, in either form. A line in `instance.env` prints a warning and is ignored |
| `TEHAS_MILLWRIGHT_APP`          | empty                                    | Optional. The login of a second GitHub App, `<slug>[bot]`, that the factory trusts as [its millwright](/operating#the-millwrights-grant) in place of its own app. The factory still acts as its own app; only the millwright's grant moves. A value that is not `<slug>[bot]` stops every command with 78. Empty: the millwright is the factory's own app |
| `TEHAS_CYCLE_LOGIN`             | refused                                  | Not a setting. The login of the account a cycle's token belonged to, which a host needed when a cycle ran as a second account with `GH_CYCLE_TOKEN`. The factory acts as one GitHub App and a cycle's pull requests are the app's own, so a line for this name stops every command with 78 and says `TEHAS_CYCLE_LOGIN is no longer used: the factory acts as a GitHub App.` Take the line out and run `tehas setup --new-app`. See [Moving a host that used a token](/github-app#moving-a-host-that-used-a-token) |
| `TEHAS_GITHUB_APP_ID`           | required                                 | The ID of the GitHub App the factory acts as ([tehasbot github-token](/tools-github#tehasbot-github-token)). `tehas setup` writes it. The installation and the bot's login are looked up and kept in `~/tehas/state/github`, not set. `TEHAS_GITHUB_APP_INSTALLATION` and `TEHAS_BOT_LOGIN` each print a warning and are ignored |
| `TEHAS_OWNER`                   | empty                                    | Optional. One person's GitHub login. When the dispatcher marks an issue `tehas:running` for a cycle it also assigns the issue to this account, so GitHub shows it as taken (see [what happens next](/operating#3-what-happens-next)). A bot's login, `[bot]`, or more than one login stops every command with 78, and `tehas setup` refuses a login GitHub does not let be assigned on `TEHAS_REPO`. Empty: nobody is assigned |
| `TEHAS_MILLWRIGHT_MAX_RETRIES`  | `2`                                      | How many cycles the millwright may start on one issue since a person last approved it |
| `TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS` | `3`                                  | How many planning questions the millwright may answer in one session. It never answers two in a row (see [the millwright's grant](/operating#the-millwrights-grant)) |
| `TEHAS_MILLWRIGHT_FEED`        | `0`                                      | 1 lets the millwright's `tehas:ready` start the first cycle of an issue a person wrote (see [the millwright's grant](/operating#the-millwrights-grant)). Anything but 0 or 1 stops every command with 78 |
| `TEHAS_MILLWRIGHT_MAX_FEEDS`   | `2`                                      | How many cycles the feed started may run at once. A positive whole number |
| `TEHAS_MILLWRIGHT_EPICS`       | `0`                                      | 1 lets a person's `tehas:ready` on an epic approve its open sub-issues for the millwright (see [the millwright's grant](/operating#the-millwrights-grant)). Anything but 0 or 1 stops every command with 78 |
| `TEHAS_REFRESH_CONFLICTS`      | `0`                                      | 1 has the factory resolve conflicts in its own pull requests that wait for a merge (see [refreshes](/operating#refreshes)). Anything but 0 or 1 stops every command with 78 |
| `TEHAS_EASY_MERGE`             | `0`                                      | 1 lets the dispatcher choose an easy merge for a refresh start instead of the full refresh cycle (see [refreshes](/operating#refreshes)): one merge of the branch into the pull request with no model and no container, at most once since the person's approval. The full refresh runs when the setting is off, when that approval has had its easy merge, when the store names no row for an easy merge it says the approval had, when nothing ran on record after the last easy merge, or when a check named by `TEHAS_BRANCH_CHECK` failed on the pull request after it. Today the choice is on the approval record, in `dispatch.log` and in the comment on the issue: the cycle an easy start runs is still the full refresh, so no host has merged this way yet. Anything but 0 or 1 stops every command with 78 |
| `TEHAS_READ_ONLY`              | `0`                                      | 1 writes nothing to GitHub: every write of the host and of a cycle's container is refused and said, and the dispatcher keeps its starts on the host (see [read-only](/operating#read-only)). Anything but 0 or 1 stops every command with 78 |
| `TEHAS_REFRESH_MAX`            | `2`                                      | Refreshes of one pull request since the person's approval its cycle rests on. A positive whole number |
| `TEHAS_REFRESH_EVERY`          | `10`                                     | Minutes between two runs of `tehasbot refresh`, from 1 to 1440 |
| `TEHAS_REFRESH_SLOTS`          | `1`                                      | Refreshes side by side in a lane of their own, from 0 to 8. They do not count against `TEHAS_TICKET_CONCURRENCY`, and new cycles leave room in the slice for them (see [refreshes](/operating#refreshes)). 0 has refreshes share the cycles' slots |
| `TEHAS_CLEAN_AFTER`            | `24`                                     | Hours a thing must have gone unused before `tehasbot clean` removes it: build output in a kept workspace, Docker's build cache and host scratch. A whole number of at least 1, else every command stops with 78 (see [tehasbot clean](/tools-clean#tehasbot-clean)) |
| `TEHAS_WORKSPACE_KEEP_DAYS`    | `7`                                      | Days a kept workspace of an open issue waits after its last use before the dispatcher removes it, with its `.images`, `.state`, transcripts and console. A closed issue's go at once. A whole number of at least 1, else every command stops with 78 |
| `TEHAS_DISK_WAIT`              | `10`                                     | The seconds one read of the host's space may take: `df`, `docker info` and `docker ps`, and one walk of `tehas disk --full` over one place at twice that. A call not answered in the time is no answer, and its figure is null with the reason rather than a command that stops; a tree that stopped answering is no figure and its place says `cut: time`. A whole number from 1 to 99. The dispatcher's hold, the previews' measure and `tehas disk` read under it (see [`tehas disk`](/api#tehas-disk)) |
| `TEHAS_CLEAN_MIN_FREE`          | `30g`                                    | Free disk space below which `tehasbot clean` takes the build output of a kept workspace and Docker's build cache whatever their age, instead of waiting for `TEHAS_CLEAN_AFTER` hours: the smaller of the file system of `~/tehas` and the one Docker keeps its images on, read with `df`. Space that cannot be read changes nothing, and a lock a cycle holds is never removed under. Host scratch and the old images keep the age. It sits above `TEHAS_CYCLE_MIN_FREE`, so space goes before the poll stops starting cycles. A size such as `512m` or `30g`, else every command stops with 78 (see [tehasbot clean](/tools-clean#tehasbot-clean)) |
| `TEHAS_CYCLE_STOP_FREE`         | `5g`                                     | Free disk space below which a running cycle is stopped, read with `df` on the file system of `TEHAS_SCRATCH_DIR` every 10 seconds by tehas work. Only the cycle whose scratch is the largest there stops, at the cap `disk`; the others go on. No figure stops nothing. Only with `TEHAS_SCRATCH_DIR`. A size such as `512m` or `5g`, else every command stops with 78 |
| `TEHAS_SCRATCH_DIR`             | unset                                    | Optional. Where a cycle's `/tmp` is kept, in place of a tmpfs of half the container's memory: each cycle gets `<dir>/<name>-<issue>` mounted there, mode 1777, and removes it when it ends, and the hourly cleaner takes what a killed cycle left. Give it a file system of its own, normally `/mnt/scratch` mounted `nosuid,nodev`, so a runaway cycle fills that disk and not the factory's: the host decides what stands behind the path. Unset, `/tmp` stays the tmpfs. Not there, no cycle starts. A planning turn and a `tehas dev` session keep the tmpfs. An absolute path of `/`, letters, digits and `._-` with no empty or dot segment and no `..`, else every command stops with 78. Its two bounds are `TEHAS_SCRATCH_MAX` and `TEHAS_CYCLE_STOP_FREE`, and they work only with this set (see [the cleaner](/operating#the-cleaner)) |
| `TEHAS_SCRATCH_MAX`             | `20g`                                    | What one cycle's scratch under `TEHAS_SCRATCH_DIR` may hold, measured every 10 seconds by tehas work; a cycle whose scratch holds more is stopped at the cap `disk`. A scratch of more than 500,000 files and directories is not read to the end and counts as over the cap, and as the largest under `TEHAS_CYCLE_STOP_FREE`. Only with `TEHAS_SCRATCH_DIR`. A size such as `512m` or `20g`, else every command stops with 78 |
| `TEHAS_CLEAN_DIRS`             | `target node_modules __pycache__ .venv .pytest_cache .mypy_cache .next .turbo .gradle` | The directory names `tehasbot clean` removes anywhere in a kept workspace, separated by single spaces. Names, not paths: one with a `/`, a bare `.` or `..`, or a dot-name in the family of the two a workspace keeps, stops every command with 78 (see [tehasbot clean](/tools-clean#tehasbot-clean)) |
| `TEHAS_BRANCH_CHECK`           | empty                                    | Optional. The name of a check that runs on every push to `TEHAS_BRANCH`, as `verify`. While it failed on the branch's head commit no new cycle starts (see [Limits that hold work back](/operating#limits-that-hold-work-back)). Words of letters, digits and `._()/-` separated by single spaces, else every command stops with 78. Empty: the poll does not ask |
| `TEHAS_ISSUE_TYPES`            | empty                                    | Optional. `kind=Type` pairs separated by single spaces, such as `improvement=Improvement prototype=Prototype`: the organisation's issue type that marks a kind in place of its label, `tehas:improvement` for the improvement loop and the dispatcher, `tehas:prototype` for the Protolab. A kind left out stays its label. Another kind, a kind named twice, or a type name of anything but letters, digits, `-` and `_` stops every command with 78; a type the organisation does not have stops the poll, the loop and the Protolab with 78 (see [Kinds as issue types](/operating#kinds-as-issue-types)). Empty: the labels |
| `TEHAS_WORK_REPO`               | `~/tehas/repos/<name>`                   | The product checkout cycles are cloned from. Must be a clone of `TEHAS_REPO`. A cycle works in `~/tehas/work/<name>-<issue>`, which is not a setting |
| `TEHAS_BRANCH`                  | the checkout's `origin/HEAD`             | Branch cycles start from and pull requests target               |
| `TEHAS_UPDATE_FROM`             | `release` or `git`, what the host has    | Where `tehas update` takes new versions from: `release`, a published release unpacked into `~/tehas/versions`, or `git`, the platform checkout, for a host that develops tehas. Unset, a host with `~/tehas/current` uses releases and one with a platform checkout uses the checkout, so a host installed before the setting existed goes on as it did; a host with neither stops with 78 and names the setting. Another word stops every command with 78 (see [tehas update](/tools-update#tehas-update)) |
| `TEHAS_RELEASE_REPO`            | the line in `versions.env`               | Optional. `owner/name` on GitHub the releases are read from, for an instance that updates from releases and takes them from another repository than the platform's own. The platform's is the line of the installed `versions.env`; the instance's wins. The list and the download use `gh` when it is signed in, which reaches a private repository with the bot's token, and `curl` from the public address otherwise. Neither names a repository, and `tehas update` stops with 78 naming both files (see [From a release](/tools-update#from-a-release)) |
| `TEHAS_CONCURRENCY`             | `1`                                      | Workstreams side by side inside one cycle. Above 1 only with `TEHAS_TICKET_CONCURRENCY=1` (or with the setting left out): with both above 1 a command stops with 78 (see [The host](/host#limits-and-concurrency)). `TEHAS_TICKET_CONCURRENCY=auto` asks for more than one cycle, and the settings check refuses the word for either count today, so `TEHAS_CONCURRENCY` above 1 goes with a number (see [The host](/host#limits-and-concurrency)) |
| `TEHAS_TICKET_CONCURRENCY`      | the host's CPUs minus one, at least 1 | The most cycles side by side: a whole number. `auto` is not a value the setting takes today: the settings check refuses it with 78 and one line that says `expected a whole number (auto is not usable yet)`, until the cycle can use the count the poll decides by. A ceiling, never a divisor: the dispatcher starts fewer while `tehas.slice` has no room, and never more than leave `TEHAS_SLICE_CONTAINERS` containers in it (see [Limits that hold work back](/operating#limits-that-hold-work-back)). A value above the host's CPUs is accepted. A container's share of the CPUs is the host's CPUs less one over this, plus `TEHAS_REFRESH_SLOTS` with refreshes on, counted at most `TEHAS_SLICE_CONTAINERS`; its pids limit and its runs of the gate at once follow the share. Its CPU quota is the host's CPUs less one whatever this is. Lower it when the model backends cannot serve this many cycles; nothing measures that. The dispatcher would answer `auto` instead: on every poll it would take this host's CPUs over `TEHAS_CYCLE_CPUS`, capped by `TEHAS_CYCLE_CEILING`, so a VM that gained or lost CPUs needs no setting changed. A running cycle is never stopped by what it says: a shrink only holds new starts. `tehas status` and the [status object](/api#tehas-status--json) would show the count in force and the word for what bounded it |
| `TEHAS_CONTAINER_MEMORY`        | `8g`                                     | What one container would be given if `TEHAS_SLICE_CONTAINERS` ever took `auto`: the dispatcher would divide `tehas.slice`'s `MemoryMax` by this on every poll, so the containers follow the host's memory. A Docker size of at least 6m, which the poll checks and `tehas status` names: a size it cannot read leaves the slice at its floor of two containers. The settings check refuses `auto` for the count today, so no instance can be in that state, and nothing else changes when this is set |
| `TEHAS_CYCLE_CPUS`              | `4`                                      | What one cycle would be given if `TEHAS_TICKET_CONCURRENCY` ever took `auto`: the host's CPUs over this are the cycles, capped by `TEHAS_CYCLE_CEILING`. A share to count with, not a quota: a container's CPU quota stays the host's CPUs less one whatever this is. A positive whole number, which the poll checks; no count there leaves the host at its floor of one cycle. Read only under `auto`, which the settings check refuses for the count today |
| `TEHAS_CYCLE_CEILING`           | `16`                                     | The most cycles `auto` may take, so a host with 96 CPUs does not ask for more cycles than the model endpoint serves. It bounds `auto` only, and `auto` is refused for both counts today: a number for `TEHAS_TICKET_CONCURRENCY` or `TEHAS_SLICE_CONTAINERS` is taken as written. A positive whole number, which the poll checks; no count there leaves the bound at 16. Nothing measures the endpoint; this is the number a person gives it |
| `TEHAS_MAX_AWAITING`            | `TEHAS_TICKET_CONCURRENCY` plus 2        | Open `tehas:awaiting-merge` issues, plus running cycles, allowed before holding |
| `TEHAS_CYCLE_RESERVE`           | `2g`                                     | Memory one container is given as a floor (`docker --memory-reservation`) and the unit the dispatcher counts room in. A reclaim priority inside `tehas.slice` and the amount admission promises, not a guarantee against the whole host. A Docker size of at least 6m, and at most the slice's `MemoryMax` over `TEHAS_SLICE_CONTAINERS`, else the command stops with 78 naming both numbers. With the defaults the host needs about 11 GiB of memory; a smaller host sets it lower |
| `TEHAS_SLICE_CONTAINERS`        | `4`                                      | Containers `tehas.slice` holds at once, from 2 to 16: a whole number. `auto` is refused with 78 like any other word until the cycle can use the count the poll decides by. Each container's memory ceiling is the slice's `MemoryMax` over this, and the dispatcher starts no cycle while this many containers are in the slice, so the ceilings never sum to more than the slice. Every container counts: cycles, refreshes, dev sessions, planning turns, the indexer. A higher value runs more cycles side by side, each with a lower ceiling: raise the host's memory with it. With `TEHAS_TICKET_CONCURRENCY=5` and one refresh slot, 6 on a 64 GiB host gives each container 8 GiB. The dispatcher would divide the slice's `MemoryMax` by `TEHAS_CONTAINER_MEMORY` on every poll for `auto` instead, between 2 and 16: a host given more memory would hold more containers with no setting to change |
| `TEHAS_WORK_TIMEOUT`            | `21600`                                  | Seconds before a cycle is given up. At most 25200: [the cycle's unit](/host#time-limits) stops a cycle after 8 hours |
| `TEHAS_WORK_STALL`              | `3600`                                   | Seconds a cycle may go without a line added to any of its logs before it is stopped as `stalled`. Must be longer than `TEHAS_VERIFY_TIMEOUT`, because a gate can print nothing while it runs; else the command stops with 78 |
| `TEHAS_WORK_IMAGES`             | `0`                                      | `1` gives a cycle and a planning session the images its issue text links: see [tehas work](/tools-work#tehas-work) and [tehas plan](/tools-plan#tehas-plan). The model must declare `"image"` in its `input` in `models.json` and answer a request with an image, and the sandbox image must say, by the label `tehas.rukas.images` its build writes, that its pi-rukas reads the manifest tehas writes beside them. Else the dispatcher holds the cycles of issues whose text links an attachment; other issues start, and `install.sh` reports what the label says |
| `TEHAS_WORK_DESCRIBE`           | `0`                                      | `1` has the model describe each of those images in words before the cycle or planning session starts, into `images.md` beside them: see [tehas work](/tools-work#tehas-work). Needs `TEHAS_WORK_IMAGES=1`, else the command stops with 78 |
| `TEHAS_WORK_FILES`              | `0`                                      | `1` gives a cycle and a planning session the text files its issue text links: md, txt, csv, json, yaml, log, html, css and js, at most 16, 1 MB each and 4 MB together. Needs no model that reads images. See [tehas work](/tools-work#tehas-work) |
| `TEHAS_WORK_COMMENTS`           | `0`                                      | `1` takes the images and files of the issue's comments too: those an allowed account wrote and nobody edited after the approval. Needs `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, else the command stops with 78. The text of the comments a retry is given does not depend on it: ticking `Retry with the issue and all its comments` is the opt-in. See [tehas work](/tools-work#tehas-work) |
| `TEHAS_WORK_ZIPS`               | `0`                                      | `1` unpacks a zip the issue's text links into the same read-only directory, at most 2 archives and 4 MB each. A zip is taken by its first bytes, not its name; every file in it goes through the checks of a linked one and counts against the same limits, and the archive is not kept. Needs `TEHAS_WORK_IMAGES=1` or `TEHAS_WORK_FILES=1`, else the command stops with 78. See [tehas work](/tools-work#tehas-work) |
| `TEHAS_VERIFY_TIMEOUT`          | `1800`                                   | Seconds one run of the product's gate may take inside a cycle. A gate that takes longer is stopped and the cycle parks with the cap `verify-timeout:<step>`. Passed to pi-rukas as `PI_ENSEMBLE_VERIFY_TIMEOUT_MS` and `PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS`; pi-rukas alone allows ten minutes. The bound is for one run under the container's CPU quota: a run that waits for another to end does not count its wait. With a `.pi/verify-cmd-fast`, the fast and the full command share it. The cycle's deadline leaves at least this much and the handoff allowance, 8 minutes, before `TEHAS_WORK_TIMEOUT`. Size it against the gate's wall time with `TEHAS_TICKET_CONCURRENCY` cycles running (see [The gate's bound and the cycles at once](/operating#the-gates-bound-and-the-cycles-at-once)) |
| `TEHAS_CHILD_TIMEOUT`           | `7200`                                   | Seconds one child of a cycle may take before pi-rukas ends it: its backstop for a child nothing else stopped. A positive whole number, at most `TEHAS_WORK_TIMEOUT`, because no child may outlive the cycle that spawned it; a longer one is refused with 78. `tehas work` passes it to the container as `PI_ENSEMBLE_SPAWN_TIMEOUT_MS`, in milliseconds; pi-rukas alone allows two hours. Raise it when a fixer still making progress is stopped with hours of the cycle's budget left |
| `TEHAS_BUILD_TIMEOUT`           | `1800`                                   | Seconds one `cargo build` of `install.sh` may take, the offline attempt and the online one each on its own. A build that runs past it is cut off, the install stops with the build's own words, and the message says the build was cut off and names the command of the attempt that was. Both programs due are built before anything is installed, so the worst case is four attempts in all: a value set with `tehas update` in mind must stay well under the 1800 s that `tehas update` gives the whole of `install.sh`, or that outer bound is the one that fires. A build cut off at the default is a host that has never fetched: run `./bootstrap.sh user` so cargo and its crate cache are there, or raise this bound. See [the host manual](/host) |
| `TEHAS_RUKAS_WORKAROUNDS`       | empty                                    | Optional. The settings of this instance that only work around a pi-rukas defect, as `NAME@VERSION` separated by single spaces, `VERSION` being the pi-rukas release that fixes it. `tehas work` stops a cycle with 78 once the image's pi-rukas is that release or later, naming the setting: remove it and its entry. An entry for a setting the instance does not set stops every command with 78. Write the pi-rukas issue in a comment beside the setting |
| `TEHAS_SIZE_MAX_FILES`          | unset                                    | Optional. The count of files an issue may name before the dispatcher holds it instead of starting it: see [Limits that hold work back](/operating#limits-that-hold-work-back). Unset, no issue is held for its size. A whole number of at least 1. The `sizes` figures of [tehas analytics](/analytics#size) say where to set it |
| `TEHAS_SIZE_MAX_BYTES`          | `131072`                                 | Bytes of an issue body `tehas work` measures when it records the size of an approved start in `state/sizes.jsonl`. The measure's pass is not free and a body may be a paste of anything, so a body past the bound is measured from its front. A whole number of at least 1 |
| `TEHAS_SIZE_MAX_LINES`          | `1000`                                   | Lines of an issue body the same measure reads. The pass costs more the more lines it holds, whatever their bytes, so the bound on bytes alone leaves a body of many short lines unbounded and this one leaves a body of one huge line unbounded: a start needs both. A whole number of at least 1 |
| `TEHAS_POLL_BUDGET`             | `cpu=5,wall=30,writes=0`                 | The cost of one poll that started nothing: the medians of the idle polls of the last hour over it make `tehasbot health` warn (`checks.poll_cost`). `cpu=<seconds>,wall=<seconds>,writes=<count>`, any part left out, each figure a whole number of at most 9 digits with no leading 0, else every command stops with 78. `tehasbot health` loads the settings and gives the value to the console with
`--budget`, which gives it to `store poll-idle --budget` as it stands, so the
rule is the store's own: what it would refuse stops the load instead of leaving
the check out in silence (see [What a poll costs](/operating#what-a-poll-costs)) |
| `TEHAS_PLAN_MAX_TURNS`          | `20`                                     | Turns a planning session may take before it is ended            |
| `TEHAS_PLAN_TIMEOUT`            | `7200`                                   | Seconds before one planning turn is given up. At most 7200, 2 hours: the unit of a turn runs to that limit plus a quarter of an hour (see [the host manual](/host#time-limits)) |
| `PI_ENSEMBLE_HOST_ALIASES`      | empty                                    | `name:address` pairs the sandbox must resolve, comma separated  |
| `PI_ENSEMBLE_REVIEW_THRESHOLD`  | unset                                    | Optional. The code-review severity that blocks a change in a cycle: `CRITICAL`, `HIGH`, `MEDIUM` or `LOW`. Read by pi-rukas, not by tehas, and it wins over the severity the product's doctrine (`AGENTS.md` or `CLAUDE.md`) states. Unset, or anything other than those four words: pi-rukas ignores it and uses the severity that doctrine states, and when it states none pi-rukas's default: MEDIUM up to v0.14.49, HIGH from v0.14.50. tehas does not check this value and does not stop with 78 over it. A `CRITICAL` finding blocks whatever this says |
| `PI_ENSEMBLE_IMAGE`             | `tehas/pi-rukas:uid<uid>`                | The sandbox image. Only its tag is the instance's to change: `sandbox/build.sh` tags what it builds under the default repository, and `tehasbot clean` takes the dated tags of that name, so a setting with another repository leaves the build's dated tags uncleaned |

A line named `PI_ENSEMBLE_*` reaches a cycle's container as it is, except for
the names tehas owns. The same line reaches a planning session and a `tehas dev`
session.

- `PI_ENSEMBLE_IMAGE` and `PI_ENSEMBLE_HOST_ALIASES`: tehas reads these and does
  not pass them on. They pick the image the container runs and its `--add-host`
  lines.
- `PI_ENSEMBLE_NO_DOCKER_SOCKET` and `PI_ENSEMBLE_NO_SSH`: switches of
  pi-rukas's own docker wrapper, which tehas does not use. tehas sets both and
  passes neither on.
- `PI_ENSEMBLE_SANDBOX_MODE` and `PI_ENSEMBLE_WORKSPACE_ROOT`: tehas sets these
  for every container. An instance line for one of them is a second `-e` for a
  name tehas has already fixed, given after tehas' own.
- `PI_ENSEMBLE_MAX_WORKSTREAMS`, `PI_ENSEMBLE_VERIFY_TIMEOUT_MS` and
  `PI_ENSEMBLE_INTEGRATION_VERIFY_TIMEOUT_MS`: tehas sets all three after it
  reads the instance file, the first from `TEHAS_CONCURRENCY` and the two
  timeouts from `TEHAS_VERIFY_TIMEOUT`. An instance line for one of them is
  replaced by tehas' value before the container starts, with no message: set
  the `TEHAS_*` name instead. No instance line gives a cycle the Docker socket
  or SSH keys.
- `PI_ENSEMBLE_SPAWN_TIMEOUT_MS` is `tehas work`'s to set for a cycle, from
  `TEHAS_CHILD_TIMEOUT`: the command drops an instance line for it before it
  adds its own, so a cycle's child bound is never the instance's. There is no
  `PI_ENSEMBLE_*` name an instance sets for it.
- `PI_ENSEMBLE_EXTRA_IMAGE_DIRS` names the mounted images directory and is
  tehas' to set: `tehas work`, `tehas plan` and `tehas dev` drop an instance
  line for it whether or not that container is given attachments, so one never
  names a directory tehas did not mount (see [tehas work](/tools-work#tehas-work)).
- `CARGO_TARGET_DIR` is tehas' to set, and only for a `tehas dev` session: the
  command gives it after the shared list, so an inherited value or one a gate
  exports is replaced by tehas' path with no message. The name is not read from
  `instance.env` and there is no `TEHAS_*` name for it: the path is named once,
  by `TARGET_DIR` in `console/core/src/sandbox.rs`. A product's gate must honour
  it rather than build into the clone, because the host runs no git in a kept
  clone to tell a person what an agent left there.
- `PI_ENSEMBLE_HANDOFF_CHANNEL`, `PI_ENSEMBLE_HANDOFF_TIMEOUT_MS`,
  `PI_ENSEMBLE_CYCLE_DEADLINE`, `PI_ENSEMBLE_VERIFY_CONCURRENCY` and
  `PI_ENSEMBLE_SPAWN_TIMEOUT_MS` are `tehas work`'s to set for a cycle, and
  `PI_ENSEMBLE_CYCLE_BUDGET_MS` is not passed: an instance line for one of
  them is dropped (see [tehas work](/tools-work#tehas-work)). A planning session and a
  `tehas dev` session get none of them.

A lever pi-rukas reads under any other name needs no setting of tehas' own, and
tehas neither checks its value nor passes it on under another name. It does
check the name: before a cycle starts, `tehas work` asks the image's runner
(`pi-rukas-headless --capabilities`) which inputs it reads, and a
`PI_ENSEMBLE_*` name the container would be given that the runner does not list
as one a host sets stops the cycle with 78, naming it. Unlike a
`TEHAS_*` setting, a malformed value here is not refused with 78: it reaches
pi-rukas and pi-rukas decides what to do with it, which for
`PI_ENSEMBLE_REVIEW_THRESHOLD` is to ignore it. The channel also carries
pi-rukas levers that loosen a cycle's own guards, `PI_ENSEMBLE_PROTECTED_PATHS`
and `PI_ENSEMBLE_POLICY_JUDGE` among them. tehas sets neither, and setting one
is a person's decision on the instance, never something a cycle can do for
itself.

The example instance sets `TEHAS_GITHUB_APP_ID`, `TEHAS_HOST_ADDR`,
`PI_ENSEMBLE_HOST_ALIASES`, and, because it is public, `TEHAS_INDEXING` and the
analytics settings. It leaves the rest at the defaults.

There is no CPU or memory budget to set. The limit around every sandbox
container is one systemd slice on the host, `/etc/systemd/system/tehas.slice`,
installed once by root with `sudo ./bootstrap.sh slice`; see
[The host](/host#limits-and-concurrency). An instance that still sets
`TEHAS_WORK_CPUS` or `TEHAS_WORK_MEMORY`, the budget of an earlier version, is
refused with 78 and one line at the first command, and `install.sh --check`
reports `Instance: <directory> INVALID` with that line under it and exits 78:

```
tehas: TEHAS_WORK_MEMORY is set and this version of tehas has no memory budget to set: the limit around all containers is /etc/systemd/system/tehas.slice (see .../instance.env)
```

Delete the line. An instance with no `TEHAS_TICKET_CONCURRENCY` line goes from
one cycle at a time, the default of the earlier version, to the host's CPUs
minus one; set the line to keep a lower ceiling.

## The gates (run by the cycle, usable by hand)

A product names its gates in `.pi/verify-cmd`, `.pi/verify-cmd-full` and
`.pi/smoke-cmd`, and may name a developer's gate in `.pi/verify-cmd-fast`: the
first line that is not empty or a comment is the command. Run them by hand
from the root of a clone of the product. For the example product:

```
scripts/verify.sh --changed  # developer's: the lint, then shellcheck and the tests of what the change touched
scripts/verify.sh            # fast: the lint, shellcheck, the shell tests, the workspace's tests, the front page's tests where node is
scripts/verify.sh --full     # plus the workspace's format and lints (rustfmt, clippy); a missing node fails
```

`.pi/verify-cmd-fast` is `scripts/verify.sh --changed`. pi-rukas tells a
cycle's developer to run that command and not `.pi/verify-cmd`, and its driver
runs it and then `.pi/verify-cmd` on every tree it gates: each worktree, the
consolidated tree and the base commit. So the whole gate still runs before a
pull request opens, and CI runs it again.

CI adds two things of its own, as steps of
`.github/workflows/verify.yml`. It installs shellcheck and jq at the versions
`host/versions.env` pins, each checked against a published checksum before it
runs. And it scans the repository's lockfiles against published advisories with
OSV-Scanner, pinned there too: the name and version of every package of
`console/Cargo.lock` and of the npm trees under `host/npm/` and
`host/sandbox/npm/` is queried against the OSV database, and nothing else is
sent anywhere. A known advisory fails the run. An advisory carried on purpose is
named in `osv-scanner.toml` with its id, the reason, and the date to look again;
after that date the ignore no longer applies and the run fails until the
advisory is fixed or the date moves with a new reason. This is the one part of
the gate that reaches the network for its answers: a scan that cannot reach the
advisory database fails the run rather than reporting nothing, and an ignore
whose advisory has gone is reported as unused but still passes, so it comes out
when the dependency moves. `.github/` is a protected path, so a cycle halts
rather than changing this: see
[the parked cycle](/troubleshooting#the-cycle-parked-needs-human-attention).

A failed step shows only its last 40 lines. Each run keeps the whole output of
every step in a log of its own, `<mode>-<UTC time>-<pid>.log` under
`TEHAS_VERIFY_LOGS` (default `$TMPDIR/tehas-verify`, else `/tmp/tehas-verify`),
the newest 20 of them, and a run that failed names its log on the last line:
`log: <path>`. A tree that passed already prints the lines of that run and
keeps no new log. CI sets `TEHAS_VERIFY_FRESH=1` and runs every step: a
runner keeps its checkout between jobs, and a pass one job kept there is not
trusted by the next.

`--changed` takes the paths changed since the merge base with `origin/main`,
uncommitted and untracked files included, and hands them to
`scripts/select-tests.sh`, which runs the selection in the workspace's
`console/xtask` (`cargo run -p tehas-xtask`, offline; a selection that cannot
build runs the whole gate). A changed test file runs itself; another path runs
the test files that name it (`libexec/update`, `tehas update`, a test file
named `update_*`). A path under `console/` also runs `cargo test -p` for the
crate it is in and the crates that depend on it, from `cargo metadata`. The
tests of the console and the dispatcher run the store's program, which no
manifest says, so `console/xtask/src/select.rs` lists that link: they run with the
store, and a change to the store runs them. A path in `console/core/`, a
manifest, `console/Cargo.lock`, a path in no crate, or metadata that cannot be
read runs the whole workspace's tests. A changelog fragment, a path under
`changelog.d/`, runs the test files that name `changelog.d`.
The heavy integration files, `test/install_*`, `test/update_*`,
`test/pack_release_test.sh` and `test/bootstrap_test.sh`, run only when the
change touches what they test directly, and a `note` line names the ones left
to the whole gate. A change to `test/lib.sh`, `scripts/test.sh`,
`scripts/verify.sh` or the selector (`scripts/select-tests.sh` and
`console/xtask/`), a path no test names, or a checkout
without `origin/main` runs the whole gate. Records under `docs/` and the
top-level notes that no test names need no test. The selection is by name, so
a test that builds a file's name at run time is missed: the whole gate after it
is what catches that.

When the shellcheck step refuses a file, the severity floor it runs at and the
one suppression it accepts are under
[`scripts/test.sh`](#scriptstestsh) below.

## scripts/test.sh

The checks of the platform itself. Run from the platform checkout; it is not
installed on a host.

```
scripts/test.sh [path...]
```

It runs shellcheck over the shell scripts, one process per file, together with
every `test/*_test.sh` in the same pool: nothing waits for the lint to finish.
Those need bash, git, jq and shellcheck, and cargo for the store (below): no
sandbox image, no product checkout and no network. At most `TEHAS_TEST_JOBS` tasks run at once: by
default the smaller of the CPUs `nproc` counts and the CPU quota of the cgroup
it runs in, rounded up, and at most two tasks a GiB of that cgroup's memory
ceiling, so the 4 GiB of `tehas check-product --sandbox` runs 8 on any host. The slowest files start first, from a short fixed list
in the script, so none of them starts last and holds the run; the rest follow
in name order and the lint of each file after them. While the host's load
average is at least its CPUs, as when other runners share the machine, the
next task waits for one to end, and one task always runs. The timeouts below
then measure a file's own time, not a queue.

Before the pool starts, it builds the state store (`console/store`) once,
offline, into the workspace's own target directory (`CARGO_TARGET_DIR`, or
`console/target`) with the workspace's dev profile, and tells every test file
where the program is (`T_STORE_BIN`) or why there is none (`T_STORE_WHY`).
The gate's `cargo test`, which `scripts/verify.sh` starts at the same time,
builds the same crates there: cargo holds the directory's lock only while it
builds, so one build waits for the other and reuses what it made, and the
tests of each run side by side. One tree instead of two takes about 170 MB
off every checkout (#1360); the dev profile keeps line tables and no
incremental data, so the tree stays under 750 MB. A `console/target/shell`
left by an earlier runner is removed. The tests of the commands that run the store (`test/dispatch_*`,
`test/cycle_*`, `test/work_*` and `test/store_approval_test.sh`) install it in
their scratch home and fail without it, so they need cargo too; the build needs the
workspace's crates in cargo's cache, as the gate's own `cargo test` does. When
the offline build fails, the runner fetches the locked crates once and builds
again: a cycle's container starts with an empty registry, and the lock can name
crates newer than the image. Without a network the fetch fails and the offline
build's reason is the one the test files get. A test file run alone builds the
store itself, into the same directory.

With paths, relative to the root of the checkout, only those run: the test
files among them and the lint of those of them it lints anyway. A test file
named that does not exist stops it with 2; paths with nothing to test or lint
among them pass with a line that says so. `scripts/verify.sh --changed` calls
it this way.

The script is a short exec of `xtask test` (`console/xtask/src/run.rs`), which
it builds first, offline, into the same target directory, fetching the locked
crates once when cargo's cache lacks one; `TEHAS_XTASK` names a build to use
instead. So the runner needs cargo, as the gate does. Each check of the pool
is an `xtask test-task` process that runs it under the timeout and records its
status and time when it ends.

Every task runs at the core limit of 0, which the run puts in front of the
check with `prlimit --core=0`, so a program that crashes during a run leaves no
core file in the checkout. The dump would land in the directory the task runs
in, which is the checkout, and a file the kernel wrote there is a file a later
check of the tree reads: a factory's gate failed on dozens of `core.<pid>`
files no test had meant to write. The limit is inherited by every process a test
file starts, which is where the crashes are. At the end of every run the
checkout is read for the files the limit meant to prevent, and one standing
there is a failure of the run: a host whose `prlimit` did not take effect is
named rather than passed. `TEHAS_TEST_CORES` set to anything keeps the dumps
instead, which is what a person sets to see what a crash really leaves; the run
then names the files the checkout holds and fails. A dump is the name the
kernel writes, `core` or `core.<pid>`: a file that merely shares the name is
not one, so `core.txt` stays a file a test meant to write. What the kernel
writes is its own file, `/proc/sys/kernel/core_pattern`, and a host that pipes
the dump to a program or writes it under a path of its own has no dump of the
checkout for any run to name; `TEHAS_TEST_CORE_PATTERN` stands in for that file,
for a run that must say what another host would have done. A host with no
`prlimit` cannot run the checks: the run says so and stops with 2, as it does
for a missing cargo, because every task of it would end `126`.

Each task is stopped after `TEHAS_TEST_FILE_TIMEOUT` seconds (300 by default)
and counts as a failure, named with `timed out`. A file that hangs therefore
costs the timeout and nothing more, and the run still prints its block.

Each run keeps everything under one root of its own,
`${TMPDIR:-/tmp}/tehas-tests/run.*`, and gives it to the test files as their
`TMPDIR`, so each file's scratch directory and the run's records are there.
The run removes the root when it ends. It holds a lock in the root, and so does
every process it starts, which inherits the descriptor: a root whose lock no
process holds is one a killed run left, and the next run removes it when it
starts. A file the timeout stopped before its own cleanup therefore leaves
nothing past the run.

Each file's output is printed whole, in name order, once all the tasks have
ended. The run then says what it covered, the total number of cases over all
the files and the slowest of them with their times (five of them by default, as
many as `TEHAS_TEST_SLOWEST` names), and ends with a block that says what did
not pass:

```
test.sh: 431 cases in 96 files, 16 at a time
test.sh: the slowest files:
      8.4 s  test/work_test.sh
      6.1 s  test/dispatch_queue_test.sh
test.sh: failed checks (2), not run (0)
    shellcheck
FAILED: test/plan_test.sh: a case that did not pass
```

The case total is the number to compare between runs: it does not go down
unless cases were removed. The slowest files are the ones to split.

Budget: 700 s for the whole gate (`scripts/verify.sh`) on the reference machine
at the commit of #1801: `nproc` counts 48 CPUs, the cgroup's quota is 47 and its
memory ceiling 12 GiB, so `TEHAS_TEST_JOBS` at its default runs 24 at a time.

Every severity of shellcheck note fails the run: error, warning, info and style.
The lint is called with no `-S` floor and no codes excluded, so a style note on a
shell script is as much a failed check as an error, and a platform checkout is
kept clean at that level. Without shellcheck the lint is a `SKIPPED:` line and
the run passes on the test files, except in CI (`CI` set) or with
`TEHAS_TEST_STRICT` set, where the lint counts as failed and the run fails, as
a missing rustfmt or clippy does in `scripts/verify.sh`. The one accepted
suppression is a note shellcheck gets wrong: on a line whose single quotes are
the point, an assertion that some text is a literal, it says an expansion was
meant (`SC2016 (info)`). Silence that one code, with the reason beside it, and
nothing else:

```bash
# shellcheck disable=SC2016  # snippets are expanded by the shell under test
```

Write that line above the code it speaks for, or at the top of the file when
the whole file is built from snippets, as the test files in this checkout do.
Nothing else is accepted: not a directive that names no code, not a second
directive beside that one, and no `-e` or `--exclude`, `--enable` or
`--source-path` on the lint's call. Each of them silences notes a person has not
read yet.

Its first line counts the checks that failed and the checks that did not run.
Each line after it names one of them: `shellcheck`, or the path of a failing
test file relative to the checkout, with a reason after it when there is more to
say than the exit status: `it did not finish, no result recorded` for a file
that never ended, `timed out after 300s` for one the timeout stopped, `its
recorded result could not be read` for a record that is
not a number, `its output could not be read` for a log the run cannot show,
`ended before t_done` for a file that exited 0 without the count line t_done
prints (`<name>: N cases, M failed`), so a stray `exit 0` or a sourced helper
that exits cannot pass the cases after it. Control bytes in a file name become
`?`. A line starting `SKIPPED:` in column 1 gives a check that did not run,
with its reason, so a caller can find it in a run that passed.

A failing test file whose own log carries a `FAIL:` line is named differently:
the line starts with `FAILED:` in column 1, with no indent, and carries the
first of those `FAIL:` lines after the path, so a gate that keeps only the end
of the output says which case did not pass and not only which file. The text is
cut to 200 characters and its control bytes become `?`, as they do in a name, so
one case cannot print escapes or take the block's lines with it. A file with a
reason and no `FAIL:` line of its own keeps the indented path, and one line per
file is kept either way, so the size of the block does not change.

The block is the last thing the run prints, and its size is derived from the
lines `scripts/verify.sh` keeps at the end of this step, so it fits inside them
with two lines to spare: past the cap it names the last failing test files and
counts the rest, as in `and 7 more, in the full log`, and it still names the
lint when the lint failed. A log cut short therefore still says how many checks
failed and whether the shell scripts were among them. Exit 0 when every check
passed, 1 when one failed, 2 when nothing could
be checked. `scripts/verify.sh` prints what a step wrote only for a step that
failed, so a run that ends with checks not run shows them in the gate only when
something else failed too.

`scripts/check-bash-lines.sh` is the ceiling on the factory's bash: the lines
of `bin/`, `lib/*.sh` and `libexec/` whose first line names bash may not grow
past the number in `scripts/bash-lines.max`, and new logic goes into the Rust
workspace under `console/` instead. Every run prints one line that begins
`check-bash-lines:` and names the working tree's count, the ceiling and the
room left, or the lines over. With a merge base the same line names the base's
own count, the room under it and what the change adds, as in `base 21587 of
21590: 3 lines of room; this change +13`, so a run can plan its bash before it
writes any: the ceiling is still judged by the working tree's figure, and a
change that took lines out keeps its minus. When the tree differs from HEAD the
line says the figure includes uncommitted work, so a tree with work in it does
not read as if the base had that room. Over the ceiling the run adds the files
that changed since the merge base with their deltas, and a `pay from` list: the
five largest counted files the change did not touch, which is where the lines
can come from, since a file the change grew is where they went in. Without a
merge base it lists the largest files instead. Exit 0 at or under the ceiling,
1 over it or when `scripts/bash-lines.max` holds no number.

`scripts/check-generic.sh` is the lint that keeps private network addresses out
of every file, and the names of particular installations out of every file,
`docs/` included. The names come from `scripts/check-generic.names`, a list
each working machine keeps for itself: git ignores it and it is read as data.
An `allow <path prefix>` line in the list exempts paths from the names, not
from the address rule. The names are matched against file paths too, and a
match is reported as `<path>: path names a listed name`. Without the list the
lint says `check-generic: no names list, names not checked` and checks
addresses only. With `--names-only` the address rule is skipped, for an
infrastructure repository that must hold addresses; names are still checked,
and without a list it exits 0 after saying so. With `--messages
<rev-range>` it also checks the commit messages of the range and the branch
name (`GITHUB_HEAD_REF`, else the current branch) against the names. A
link in the tree is not read. The verify workflow checks the names in a job
of its own on a GitHub-hosted runner: it writes the list from the
`GENERIC_NAMES` secret when it has one, and runs `--names-only`, with
`--messages` on a pull request. On a pull request that job runs the base
branch's `check-generic.sh`, not the pull request's. The jobs on the
organization's own runners never get the list, so their run of the lint
checks addresses only. `scripts/verify.sh` runs the
lint and `scripts/test.sh` together.
## The pages

The text of each command is on the page named for it. The list above,
`## Instance settings`, `## The gates (run by the cycle, usable by
hand)` and `## scripts/test.sh` stay here.

| Page | What is on it |
| --- | --- |
| [tehas dispatch](/tools-dispatch) | The one poll of the issue queue: who may start work, what a poll costs, and what holds work back. |
| [tehas work](/tools-work) | One cycle for one issue, run by hand: its container, the links its issue carries, and its report. |
| [tehas plan and tehasbot review](/tools-plan) | One turn of a planning session, and one review of a branch. |
| [tehas dev and tehas index](/tools-dev) | An interactive session on the host, and the code index the agents search. |
| [tehas setup, labels, check-product and init-product](/tools-setup) | Writing this host's instance and starting the factory, and the checks of the product's contract. |
| [The commands that hold work back](/tools-control) | An outside cap, a drain, the records of what ended, and stopping a cycle by hand. |
| [tehas status, api, disk, top and analytics](/tools-status) | What the factory is doing, what it speaks to a program, how full the host is, and its runs. |
| [tehasbot and tehasbot cycle](/tools-tehasbot) | The command the units run, and one cycle for one issue with its report. |
| [The console, health, energy and top](/tools-health) | The files that feed the dashboard, and what watches the host's health, its models and its load. |
| [The Protolab and the improvement loop](/tools-protolab) | The page of a prototype, and the issues the factory files about its own cycles. |
| [The GitHub commands of tehasbot](/tools-github) | Asking for refreshes, the App's tokens, and the writes of the last day. |
| [tehasbot clean and tehasbot preview](/tools-clean) | What the cleaner removes and when, and the preview of every pull request. |
| [tehas update, install.sh and bootstrap.sh](/tools-update) | Bringing a host up to date, installing one, and preparing a fresh one. |
| [Releases and polyfills](/tools-releases) | The tarball, its notes, its pins and the workflow that makes it, and the polyfills of pi-rukas features. |
