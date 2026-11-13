---
title: A GitHub App for a factory
description: "Making the GitHub App a tehas factory acts as, instead of a machine user."
---

# A GitHub App for a factory

A factory acts on GitHub as one identity: it labels, comments, pushes branches
and opens pull requests. That identity is a **GitHub App**, one app for
everything this factory does. A machine user (a second GitHub account with a
personal access token) is not used: GitHub can suspend a user account that
automation drives hard, and then everything the account wrote is hidden and
every poll fails. That happened to tehas-tehas on 2026-10-24.

An app is not a user. Its installation tokens last one hour, reach only the
repositories it is installed on, and its writes show as `<slug>[bot]`. tehas
makes and renews the tokens itself: nothing on the host holds a long-lived
GitHub token. The factory is named by one setting, `TEHAS_GITHUB_APP_ID`. The
installation and the bot's login are looked up at GitHub and kept in
`~/tehas/state/github`; they are not settings.

## Make the app with tehas setup

```bash
tehas setup --new-app
```

Setup prints one link. Open it in a browser signed in to the account that
should own the app: GitHub shows the app's name and permissions, filled in.
Press **Create GitHub App**. GitHub then opens a page whose address ends in
`?code=...&state=...`. Paste that address (or the code alone) at setup's
prompt. Setup turns the code into the app, writes the key to
`~/.config/tehas/github-app.pem` (mode 600) and the app's ID to `instance.env`
as `TEHAS_GITHUB_APP_ID`, and prints the install link. Install the app on the
product repository, and on the repository improvement findings go to if it is
another one. Setup waits up to 10 minutes for that, then goes on.

`--app-owner <login>` sets the organisation or user that owns the app (the
repository's owner by default) and `--app-name <name>` its name
(`<repository>-factory` by default; the slug of the name becomes the bot's
login). The code lasts an hour and works once. Run `tehas setup` again to
start over.

To use an app you already have, give its ID and key:

```bash
tehas setup --app-id <App ID> --app-key-file <downloaded>.private-key.pem
```

[tehas setup](/tools-setup#tehas-setup) lists every step. Setup ends with
`tehasbot github-token --check`.

## Make the app by hand

Do this on a host that was set up before the wizard, or to see what the wizard
asks for. In the organisation (or account) that owns the product repository:
**Settings, Developer settings, GitHub Apps, New GitHub App**.

| Field | Value |
| --- | --- |
| GitHub App name | `<product>-factory`, for example. Its slug becomes the bot's login, `<slug>[bot]` |
| Homepage URL | The product repository's URL. Nothing calls it |
| Callback URL, Setup URL | Empty |
| Expire user authorization tokens | Leave on. tehas uses installation tokens, which always last one hour |
| Request user authorization (OAuth) during installation | Off |
| Enable Device Flow | Off |
| Webhook, Active | Off. tehas polls; it needs no public address |
| Where can this GitHub App be installed? | Only on this account |

**Repository permissions**, everything else "No access":

| Permission | Access | What tehas does with it |
| --- | --- | --- |
| Contents | Read and write | Push cycle branches, read the code and releases |
| Issues | Read and write | Labels, comments, issue edits, closing |
| Pull requests | Read and write | Open, edit and ready pull requests |
| Metadata | Read-only | Required by GitHub |
| Actions | Read-only | Read CI runs |
| Checks | Read-only | Read a pull request's checks |
| Commit statuses | Read-only | The same, for statuses |

Leave **Workflows** at "No access" on purpose: with it off, no cycle can push a
change to `.github/workflows/`. No organisation or account permissions, and
subscribe to no events.

Create the app. On its **General** page note the **App ID**, and under
**Private keys** generate one: a `.pem` file downloads. Keep it secret; it is
the app's only credential. Install the app (**Install App**, then **Install**
next to the organisation) on the product repository, and on the repository
improvement findings go to if it is another one.

Then give the app to the host with `tehas setup --app-id <App ID>
--app-key-file <key>`, or by hand as the tehas user:

```bash
install -m 600 <downloaded>.private-key.pem ~/.config/tehas/github-app.pem
```

and one line in the instance's `instance.env`:

```bash
TEHAS_GITHUB_APP_ID=<App ID>
```

The key stays in `~/.config/tehas`, outside the instance directory, and is
never given to a cycle's container. The installation ID and the bot's login
are not written down: the first command that needs them asks GitHub with the
app's key and keeps the answers in `~/tehas/state/github/installation` and
`~/tehas/state/github/login`. If the app is not installed on `TEHAS_REPO`,
every command says which repository and gives the install link.

Commits a cycle makes are authored as the tehas user's git identity. `tehas
setup` sets it to the app. By hand, the bot's user ID is
`gh api 'users/<slug>[bot]' --jq .id`:

```bash
git config --global user.name '<slug>[bot]'
git config --global user.email '<user id>+<slug>[bot]@users.noreply.github.com'
```

Then check it:

```bash
. ~/tehas/env.sh
tehasbot github-token --check
```

It prints the app, the installation, the bot's login, when the token runs out
and whether the installation reaches `TEHAS_REPO`, and never the token itself.
`command -v gh` now answers `~/tehas/shim/gh`.

## Moving a host that used a token

`GH_TOKEN` in `~/.config/tehas/env`, `GH_CYCLE_TOKEN` and `TEHAS_CYCLE_LOGIN`
are refused: any command stops with exit 78 and says
`<NAME> is no longer used: the factory acts as a GitHub App.` Take them out,
make or give the app as above, and run any command again.
`TEHAS_GITHUB_APP_INSTALLATION`, `TEHAS_BOT_LOGIN` and
`TEHAS_MILLWRIGHT_LOGIN` in `instance.env` print one warning each and are
ignored once `~/tehas/state/github/login` exists; take them out too.
`tehas setup --token-file` is refused, and `--app-installation` is ignored.

## What tehas does with it

- `tehasbot github-token` signs a short JSON Web Token with the key, exchanges
  it for an installation token, and keeps that in `~/tehas/state/github/token`
  (mode 600) until fewer than 10 minutes are left.
- `env.sh` puts `~/tehas/shim` first on `PATH`. Its `gh` asks for the token at
  every call, so a command that runs for hours never holds an expired one;
  git's github.com credential helper asks the same `gh`. The dispatcher's poll is the one caller
  that asks once: it takes the token at its start, gives it to every `gh` of
  the poll as `TEHAS_GH_TOKEN`, and gives it to `tidy`, `status` and `health`
  when the poll runs them, so the `gh` of none of them asks again (#938, #965).
  The poll keeps it in memory only - never in a log, a file, or the
  poll's record.
- A cycle's container is never given the app's own token. It gets a second
  token from the same app, narrowed to the product repository with Contents
  and Pull requests write and Issues read (#1814): `tehasbot github-token --cycle`
  makes it and keeps it in `~/tehas/state/github-cycle`. The container is
  given that directory read-only, and the token as its first `GH_TOKEN`. The
  cycle's pull request and comments carry the app's `<slug>[bot]` login, so no
  user account is behind a cycle and none can be suspended. Without the
  narrowed token no cycle starts. A planning turn's and a manual session's container is given a token
  of the same body, made when the command starts, renewed every 5 minutes
  while the container runs (`tehasbot github-token --session`) and kept in
  `~/tehas/state/github-session`, read-only, never the key or the app's own
  token, and the image's own `gh` reads the token there at every call.
  Neither token can label an issue or comment on one: the issue writes of a
  container are not made with it. A cycle's `gh issue create`, `gh issue edit`
  and `gh issue comment` go to the host instead, through the cycle's relay
  directory (`<workspace>.state/relay`, `/run/tehas/relay` in the container):
  `tehas work` runs them with the app's token, asked for each one, and only
  with a title, a body and an issue number. A comment goes only on the
  cycle's own issue or on one the cycle created through the relay, and an
  edit only on an issue the cycle created during that cycle. A cycle has at
  most 50 such writes. A label, an assignee, a milestone, a project, a
  state, another repository, a body over 1 MiB, a link in place of a file,
  and a title or body with `<!-- tehas:` in it, in any letter case and with
  any white space after `<!--`, are refused with the reason. A planning turn
  and a session have no relay. `tehas work` renews the cycle's token
  every 5 minutes while the cycle runs. A failed renewal is one line
  in the cycle's work log, never a token, and is tried again after 30 seconds,
  so two failures in a row no longer let the token run out. The renewer is
  `tehas-cycle renew` (`console/cycle/src/renew.rs`).
- The dispatcher, the refresh and the Protolab take the bot's login from
  `~/tehas/state/github/login`, because an installation token cannot ask
  GitHub who it is. The millwright is that same login (see
  [the millwright's grant](/operating#the-millwrights-grant)).
- When a new token finds the app renamed (see
  [the commands page](/tools-github)), the bot's user id in
  `~/tehas/state/account-ids` is recorded under the new login before
  `~/tehas/state/github/login` is rewritten; the old pair stays. If that
  record cannot be written, the old login is kept and the next token tries
  again. The dispatcher knows the bot's comments by that user id, and by
  login only when none is recorded, so a question asked before the rename is
  still found.

The shim's `gh` keeps an account of what the factory writes. A call that writes
to GitHub - a comment, a label, an `api` call that posts, patches, puts or
deletes, a GraphQL `mutation` - appends one tab-separated line to
`~/tehas/state/github-writes.log` before it hands gh's own output back: the
time in UTC, the kind (`api POST`, `issue comment`, ...), the repository, the
issue or PR number, gh's exit status, and `403` or `429` when gh's error stream
says so, else `-`. It holds nothing else: not the token, not a body, not a
field value. Reads are not logged and run as they always did. `gh api graphql`
always sends POST, a query among them, so the shim reads its method for
nothing: it reads the first word of the query instead, and logs the call only
when that word is `mutation`. An idle poll asks GitHub nothing but queries, and
so adds no line. Which call is a write, and the line, are decided by
`console gh`, which the shim execs; a read goes on to gh at once. A host whose
console is missing or cannot be run still has a working `gh`, with the token,
and logs nothing until the console is repaired. The poll line of `tehas
status` counts it, and `tehasbot github-writes` shows the last day by hour and
by its busiest minute against GitHub's limits. A write GitHub refuses for a
rate limit is sent once more after a minute, and only that call
([the commands page](/tools-github#tehasbot-github-writes)).

[The commands page](/tools-github#tehasbot-github-token) has the details.

## What a container's token may do

A cycle's, a planning turn's and a session's container get the narrowed
token, never the app's own. It has these permissions on the product
repository and on nothing else:

| Permission    | Access |
| ------------- | ------ |
| Contents      | write  |
| Pull requests | write  |
| Issues        | read   |
| Metadata      | read   |

GitHub lets a token with Pull requests write manage the repository's labels:
create, rename and delete them. GitHub's tables list Issues write or Pull
requests write for those calls. So a container can rename a label, though it
cannot apply one to an issue or comment on one with this token. The
dispatcher does not take a label's name as proof of what it is: it pins each
granting label, as the next section says.

## Label pins

The dispatcher keeps the GitHub node id of each label whose application
grants work (`tehas:ready`, `tehas:plan`, `tehas:refine`, `tehas:finish` and
`tehas:lean`) in `~/tehas/state/labels`,
one `<name><tab><id>` line each. The first page of each poll reads the
repository's `tehas:` labels with their ids, and the label events read from
GraphQL carry the id of their label:

- `tehas labels`, which `install.sh` runs and `tehas update` runs when the
  table of labels changes, makes the labels and then pins them, and prints
  `pinned the label <name> as <id>`. With no file yet it pins every granting
  label it finds, so a host that ran it at install has a pin for each. Once
  the file exists it pins only a label it made itself, one that did not exist
  before it ran: a label that was already there with no pin could be another
  label renamed to the name, so it is named (`not pinned: ... was there before
  with no pin`) and the command exits 1.
- A host with no file, one from before the pins, is pinned by its first poll
  that reads the labels: each granting label it finds, and the file is written
  even when it finds none. A dry run says what it would pin and writes
  nothing. From then on the poll pins nothing.
- A granting label with no line in the file grants nothing: its applications
  count as nobody's and the poll logs `refused: the label <name> (<id>) has no
  pin`. A label that was missing when the pins were made therefore cannot be
  brought in by renaming another label to its name.
- An application whose label id is not the pinned one counts as nobody's:
  it starts nothing, and the label is taken off as one applied by an
  account that is not allowed. The poll logs `refused: ...` with the label's
  name, the pinned id and the event's id. When the label that now carries a
  granting name is not the pinned one, no application of it counts that poll,
  and the log says `refused: the label <name> is not the one pinned`.
- A pinned label the poll's read of the labels did not name (deleted, or
  pushed off the read's page of 100 by other `tehas:` labels), and every
  pinned label when the labels could not be read, grants nothing that poll:
  label events read from REST carry no label id, so they could not be told
  from another label renamed to the name.

`tehas labels` pins `tehas:prototype` the same way, and the
[Protolab](/tools-protolab#tehasbot-protolab) publishes no ticket while the
label under that name is not the pinned one. The dispatcher does not act on
it.

A label renamed into a granting name, for example a person's `bug` renamed to
`tehas:ready`, therefore grants nothing: its old applications carry the id of
the old label.

To delete and recreate a `tehas:` label, remove its line from
`~/tehas/state/labels` and delete the label, then run `tehas labels`: it makes
the label and pins it. To pin a label that is there and has no pin, after
checking on GitHub that it is the label it should be, add its line
(`<name><tab><id>`, the id `tehas labels` printed) to the file by hand.
`tehas labels` never changes a pin that is there. Until a label is pinned its
applications count for nothing.

## The relay is a security channel

The relay is the one way a cycle's container reaches GitHub's issues with
more than read access, so it is a host channel and is held to the rule that
nothing may widen what a container is given (the mounts and variables of
`tehas work` are pinned by `test/work_test.sh`). The container gets a
directory to leave requests in. It gets no token for it and no way to ask for
anything but the three commands. The host reads each request, checks it and
runs it. It refuses, with the reason written back to the cycle:

- a command other than `gh issue create`, `gh issue edit` and `gh issue comment`;
- a label, an assignee, a milestone, a project, a state change and another
  repository;
- a comment on an issue other than the cycle's own and the ones it created;
- an edit of an issue the cycle did not create during that cycle;
- a body over 1 MiB, a link in place of a file, and any text containing
  `<!-- tehas:`, in any letter case and with any white space after `<!--`;
- the 51st write of a cycle.

A title and a body that pass are posted masked as the console masks a log:
tokens, keys, passwords, private addresses and the host's names are taken
out. Every `@` mention gets a zero-width space after the `@`, so a cycle's
text notifies nobody.

The host also takes the marker apart where it quotes an agent's text itself, in
its own comments and in the plan, so a marker an agent wrote cannot come back
as the factory's.

## Things seen on the first app

- A token's answer for a repository says `"push": false`. That field is a
  user's access and is not filled for an app; Contents write is what lets it
  push.
- GitHub names the app `<slug>[bot]` in REST answers and `<slug>` in GraphQL
  and `gh --json`. tehas drops the `[bot]` where it compares the two.
- Moving an existing factory from a machine user to an app: comments the old
  account wrote stay where they are, but if that account is suspended they are
  hidden, and the dispatcher cannot see a question it asked there
  (`skip #N: its question is not among the last 100 comments`). Apply
  `tehas:ready` or `tehas:refine` again to go on with such an issue. A
  question on record that is there but was written by another account than
  the bot is said as `skip #N: its question was written by 'X', not the bot
  'Y'`.
