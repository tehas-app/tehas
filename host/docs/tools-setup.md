---
title: tehas setup, labels, check-product and init-product
description: "Writing this host's instance, the labels of the repository, and the product's contract."
---

## tehas setup

```
tehas setup --repo tehas-app/tehas --labellers janit --token-file - \
  --endpoint http://gb1:8086/v1 <~/bot-token
```

The first setup of an instance, as the tehas user, after
[bootstrap.sh](/tools-update#bootstrapsh). Before the first install `tehas` is not on `PATH`
yet: run it from the tree it came in, `~/tehas/current/bin/tehas setup` for a
release or `~/tehas/platform/bin/tehas setup` for a checkout. What it is not
given it asks for when there is a terminal; without one, a missing setting is
exit 78.

| Option | What it is |
| --- | --- |
| `--repo <owner/name>` | The product repository |
| `--labellers "<a> <b>"` | The logins whose labels count. Never the bot |
| `--token-file <file>` or `-` | The bot's token, from a file or standard input. There is no option that takes the token itself, so it is never on a command line or in the shell's history |
| `--app-id <id>`, `--app-installation <id>`, `--app-key-file <file>` | The [GitHub App](/github-app) the factory acts as, in place of a token: the app's ID, its installation's ID and its private key. All three or none, and not with `--token-file`, else exit 64 |
| `--endpoint <url>` | The OpenAI-compatible model endpoint, with its `/v1`. Its models are read from `<url>/models` |
| `--model <id>` | The model. Needed when the endpoint offers more than one and there is no terminal to pick on |
| `--model-key-file <file>` or `-` | The endpoint's key, if it takes one. It goes in `~/.config/tehas/env` as `TEHAS_MODEL_KEY_<PROVIDER>`, and `models.json` names that variable. Curl gets it on standard input |
| `--provider <name>` | The provider's name in `models.json`. Without it, `main` |
| `--control-issue <n>` | The control issue. Without it one is made, titled "tehas control" |
| `--no-build` | Leave out the sandbox image and the product's gates |
| `--force` | Replace a setting that is there and differs |

In order, stopping at the first step that fails:

1. It checks that the token can push to the repository
   (`gh api repos/<repo>` answers `push: true`) and that the endpoint offers
   the model, then makes the control issue. Nothing is written before these.
   With an app it makes a token from the given key instead and checks that
   the token's permissions have `contents`, `issues` and `pull_requests` at
   `write` and that the installation lists the repository
   (`GET /installation/repositories`). It reads the app's slug (`GET /app`)
   and the ID of its bot, `<slug>[bot]` (`GET /users/<slug>[bot]`). A check
   that fails stops the run with 1 and nothing written. The token is kept in
   no file.
2. It writes the instance directory (`~/.config/tehas`, or the one
   `~/.config/tehas/instance-dir` names): `instance.env` with the five required
   settings and nothing else, and `models.json` with the provider's `api`,
   `baseUrl`, `apiKey` and the model's `id`. The token goes in
   `~/.config/tehas/env` as `GH_TOKEN`. The directory is mode 700, the files 600.
   With an app there is no `GH_TOKEN`: the key is installed at
   `~/.config/tehas/github-app.pem`, mode 600, and `instance.env` gets
   `TEHAS_GITHUB_APP_ID`, `TEHAS_GITHUB_APP_INSTALLATION` and
   `TEHAS_BOT_LOGIN=<slug>[bot]`. Another key or other app settings are
   replaced only with `--force`. From here `gh` runs through `~/tehas/shim/gh`,
   which reads the app from these files and keeps the account of what the
   factory writes ([GitHub App identity](/github-app)).
3. It gives git the bot's login and noreply address (`<id>+<login>@users.noreply.github.com`)
   from `gh api user`, unless git already has a name or an email, which it keeps.
   With an app the login is `<slug>[bot]` and the ID the one step 1 read,
   because an installation token cannot ask `/user`.
   It gives git `gh`'s credential helper for `github.com`
   (`gh auth setup-git --hostname github.com`) when git has none, which is what
   lets plain git read a private product's remote: the clone in step 4 and the
   fetches of [tehas dispatch](/operating), [tehas update](/operating) and
   [tehas status](/tools-status#tehas-status). The check is what git itself would use,
   every scope it reads, and `gh auth setup-git` also writes the same key for
   `gist.github.com`. A helper that is there is kept and named by its count:
   the value is a command an operator wrote and can hold a token, so it is
   named, never printed, and an entry whose value is empty is not a helper
   because gitcredentials ends the list there. A container's git is given its
   helper by environment-level configuration instead, whatever the host's
   files say. If `gh auth setup-git` fails, the run stops with 1 before the
   identity is checked.
4. It clones the product to `~/tehas/repos/<name>` when there is no checkout.
   A checkout it finds, such as one `tehas state unpack` brought from another
   host, it fetches from origin and fast-forwards on the branch it is on, with
   `--no-build` too, so the gates run on origin's code. A checkout with local
   changes, commits of its own, a detached HEAD, or a fetch that fails or takes
   over 120 seconds is left as it is: the run names the path and the reason in
   a line `no fast-forward of the product checkout <path> (<why>); the checkout
   was left as it is`, and goes on with its status unchanged.
5. It runs `install.sh` from the same tree as the command, then
   [tehas labels](#tehas-labels).
6. Unless `--no-build`: it builds the sandbox image when there is none
   (`~/tehas/sandbox/build.sh`), then runs the product's gates in it with
   `tehas check-product --sandbox` where that command is installed. Where it is
   not, it says the gates were not run.
7. It enables `tehas-dispatch.timer` and `tehas-dispatch.path`, starts the docs
   server and prints the dashboard's address.

A second run changes nothing that is right: a value that is there is kept, the
control issue is not made again and an image that is there is not rebuilt. A
setting that is there and differs is never replaced without `--force`. The run
names each one and stops with 78 before it calls GitHub or the endpoint and
before it writes anything.

The whole run is the console's (`console setup run`), and what steps 1 and 2
say goes to standard error. It writes `models.json` with its keys sorted. A
release carries the console; from a checkout that has not built one, setup
builds it with cargo first, in the few lines of bash that are left of
`libexec/setup`.

Exit 0 when done, 1 when a check or a step failed (or the step's own status,
such as 78 from `install.sh`), 64 on an unknown option, 78 for a setting that is
missing, malformed or differs, or when there is no console and no checkout to
build one from.

## tehas labels

```
tehas labels        # one line per label
```

Creates the twenty-one labels tehas uses in the product repository: `tehas:ready`,
`tehas:urgent`, `tehas:running`, `tehas:awaiting-merge`, `tehas:shipped`, `tehas:pause`,
`needs-human-attention`, `tehas:question`, `tehas:plan`, `tehas:refine`, `tehas:planning`,
`tehas:replaced` and `tehas:refined` (an issue a [refine](/operating#refining-a-request)
filed a ticket for), `tehas:millwright-escalated`, `tehas:next`, `tehas:blocked`, `tehas:refreshing` (on a
pull request a [refresh](/operating#refreshes) works on), `tehas:all-hands`,
`tehas:lean` (see [choosing the driver](/operating#choosing-the-driver)),
`epic` (see [epics](/operating#3-what-happens-next)) and `tehas:finish` (see
[finishing a stopped cycle](/operating#finishing-a-stopped-cycle)).
Run it once for a new product, and again after pulling a version that adds a
label. Run again, it puts their colours and descriptions back. The labels are
the lines of `~/tehas/lib/labels.tsv`, which `install.sh` copies from
`host/labels.tsv`: a change to that file owes the `labels` step of an update, and
a new console does not. It needs `TEHAS_REPO` and no other setting. Exit 0 when
every label is there, 1 when one could not be made, 64 on any argument, 78 when
the repository is not set or the table is missing or holds a line that is no
label, before any label is made.

## tehas check-product

```
tehas check-product             # one line per check
tehas check-product --sandbox   # and run the product's gates in the image
```

Checks the product checkout against the [contract](/new-project#what-the-product-repository-must-provide),
on the commit origin's default branch points at, since a cycle reads only what
is committed there. One line per check, `ok` or `FAIL`:

| Check     | Passes when                                                                                     |
| --------- | ----------------------------------------------------------------------------------------------- |
| `origin`  | `TEHAS_WORK_REPO` is a clone of `TEHAS_REPO`                                                    |
| `default` | `origin/HEAD` is set, so the default branch is known without the network                        |
| `agents`  | `AGENTS.md` is there                                                                            |
| `gate`    | `.pi/verify-cmd` has a line that is not empty or a comment                                      |
| `model`   | `.pi/settings.json`, when the product has one, names a model the instance's `models.json` lists |
| `install` | One line per install a cycle runs before it starts: the first command of `.pi/install-cmd`, else one for each lockfile at the root or one directory down (see [the contract](/new-project#what-the-product-repository-must-provide)) |

With `--sandbox` it then clones that commit into `~/tehas/tmp/<name>`, `<name>`
the repository's name, and works in the sandbox image as this user and in the
slice. It runs the installs at the root of the clone, then, in a worktree of the
clone as pi-rukas makes one in a cycle, `.pi/worktree-setup` when there is one,
the first command of `.pi/verify-cmd-full` (else `.pi/verify-cmd`) and the first
command of `.pi/smoke-cmd` when there is one. An install that fails, or a tool
the image lacks (`no pnpm in the sandbox image`), is a `FAIL install` line and
no gate runs. The clone and its worktree are removed afterwards; the product's
checkout gets no worktree and no branch.

It changes nothing in the product. The result goes to
`~/tehas/state/check-product.json`, which `tehas status` prints as its
`product` line and `./install.sh --check` as its `Product:` line; neither runs
the check itself, and the `Product:` line does not change install.sh's exit
status. Exit 0 when every check passed, 1 when the sandbox could not be run (no
image, or the clone failed), 64 on a wrong argument, 78 when the settings are
missing or malformed, a check failed, or no console stands beside the command.
The command loads the settings; the checks and the run are the console's
(`console check-product`, `console/src/product.rs`).

## tehas init-product

```
tehas init-product             # a branch and a pull request with the missing files
tehas init-product --dry-run   # print the files, change nothing
```

Proposes the contract files the default branch does not have yet: `AGENTS.md`
(a short template), `.pi/verify-cmd` and `.pi/worktree-setup`. A file that is
there is never written. The gate and the install step are guessed from, in
this order: a `test`, `check` or `verify` target in the `Makefile`;
`package.json` (its `test`, `check` or `build` script, with npm, pnpm, yarn or
bun by the lockfile); `Cargo.toml`; `go.mod`; `pyproject.toml` (with uv when
there is a `uv.lock`); the `run:` steps of `.github/workflows` that mention
test, check, verify or lint. Each guessed file says in a comment that a person
should check it. With no guess, `.pi/verify-cmd` holds only a comment, and
`tehas check-product` still fails its gate check.

The work is done in a fresh clone under `~/tehas/tmp`. The files are committed
to the branch `tehas/init-product`, pushed with gh's credentials, and the bot
opens the pull request against the default branch. It never commits to the
default branch. Exit 0 when the pull request is open or nothing is missing, 1
when git or gh failed or `tehas/init-product` is already on origin, 64 on a
wrong argument, 78 when the settings are missing or malformed, the checkout is
not a clone of `TEHAS_REPO`, or no console stands beside the command. The
command loads the settings and checks the checkout; the rest is the console's
(`console init-product`, `console/src/product.rs`).
