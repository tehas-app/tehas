---
title: tehas setup, labels, check-product and init-product
description: "Writing this host's instance and starting the factory, and the checks of the product's contract."
---

# tehas setup, labels, check-product and init-product

## tehas setup

```
tehas setup --repo tehas-app/tehas --labellers janit --new-app \
  --endpoint http://gb1:8086/v1
```

The first setup of an instance, as the tehas user, after
[bootstrap.sh](/tools-releases#bootstrapsh). Before the first install `tehas` is not on `PATH`
yet: run it from the tree it came in, `~/tehas/current/bin/tehas setup` for a
release or `~/tehas/platform/bin/tehas setup` for a checkout. What it is not
given it asks for when there is a terminal; without one, a missing setting is
exit 78.

| Option | What it is |
| --- | --- |
| `--repo <owner/name>` | The product repository |
| `--labellers "<a> <b>"` | The logins whose labels count. Never the bot |
| `--new-app` | Make the [GitHub App](/github-app) the factory acts as (below) |
| `--app-owner <login>` | The organisation or user that owns the new app. Without it, the repository's owner |
| `--app-name <name>` | The new app's name, up to 34 characters. Without it, `<repository>-factory` |
| `--app-id <id>`, `--app-key-file <file>` | A GitHub App you have, in place of `--new-app`: its ID and its private key. Both or neither, and not with `--new-app`, else exit 64 |
| `--endpoint <url>` | The OpenAI-compatible model endpoint, with its `/v1`. Its models are read from `<url>/models` |
| `--model <id>` | The model. Needed when the endpoint offers more than one and there is no terminal to pick on |
| `--model-key-file <file>` or `-` | The endpoint's key, if it takes one. It goes in `~/.config/tehas/env` as `TEHAS_MODEL_KEY_<PROVIDER>`, and `models.json` names that variable. Curl gets it on standard input |
| `--provider <name>` | The provider's name in `models.json`. Without it, `main` |
| `--control-issue <n>` | The control issue. Without it one is made, titled "tehas control" |
| `--no-build` | Leave out the sandbox image and the product's gates |
| `--force` | Replace a setting that is there and differs |

`--token-file` is refused with exit 78: the factory acts as a GitHub App.
`--app-installation` is accepted with a warning and ignored: the app's
installation on the repository is looked up. Run again on an instance that
names an app and has its key, setup checks that app; `--new-app` then makes
none. A key in `~/.config/tehas/github-app.pem` with no
`TEHAS_GITHUB_APP_ID`, or an app ID with no key, stops `--new-app` with 78 and
names `--app-id` and `--app-key-file`.

With `--new-app`, after the endpoint check:

- It asks GitHub whether the owner is an organisation (`GET /users/<owner>`)
  and builds the app's manifest: Contents, Issues and Pull requests write;
  Metadata, Actions, Checks and Commit statuses read; no workflows, no events,
  the webhook off; private.
- It prints one `data:text/html` link. Pasted into a browser's address bar, the
  page posts the manifest to GitHub's new-app form for the owner. The same page
  is written to `~/.config/tehas/new-app.html`, mode 600, for a browser that
  will not open the link: the file is on the host, so copy it to the machine
  the browser runs on. A link in place of the file or its directory is not
  written through.
- GitHub shows the permissions. Create the app, and GitHub opens the owner's
  page with `?code=...&state=...`. Paste that address, or the code alone, at
  setup's prompt. An address must carry the state this run made (random, 48
  hex digits); one with another state or none is refused with 78.
- It sends the code to `POST /app-manifests/<code>/conversions` (no
  authentication; the code goes to curl on standard input). A code lasts an
  hour and works once. The answer's key is written to
  `~/.config/tehas/github-app.pem` (a new file, mode 600) and the app's ID to
  `instance.env` as `TEHAS_GITHUB_APP_ID` in the same step, and the page is
  removed. The key and the app's secrets are never printed.
- It prints the install link `https://github.com/apps/<slug>/installations/new`
  and waits up to 10 minutes, asking `GET /app/installations` every 5 seconds,
  for the app to be installed. Then the run goes on as with `--app-id`.

In order, stopping at the first step that fails:

1. It checks that the endpoint offers the model. It looks up the app's
   installation on the repository (`GET /repos/<repo>/installation`); where
   there is none it names the repository and the install link. It makes a
   token from the key and checks that the token's permissions have
   `contents`, `issues` and `pull_requests` at `write`. It reads the app's
   slug (`GET /app`) and the ID of its bot, `<slug>[bot]`
   (`GET /users/<slug>[bot]`). Each other repository `instance.env` names
   (`TEHAS_IMPROVE_REPO`, `TEHAS_IMPROVE_RUKAS_REPO`, `TEHAS_REVIEW_REPO`,
   `TEHAS_RELEASE_REPO`) that the app does not reach is named with the install
   link, and the run goes on. Then it makes the control issue. A check that
   fails stops the run with 1 and nothing written, except the key and the ID
   `--new-app` wrote. The token is kept in no file.
2. It writes the instance directory (`~/.config/tehas`, or the one
   `~/.config/tehas/instance-dir` names): `instance.env` with the five required
   settings and `TEHAS_GITHUB_APP_ID`, and `models.json` with the provider's `api`,
   `baseUrl`, `apiKey` and the model's `id`. The directory is mode 700, the
   files 600. A given key is installed at `~/.config/tehas/github-app.pem`,
   mode 600, and `instance.env` gets `TEHAS_GITHUB_APP_ID`. Another key or
   another app ID is replaced only with `--force`, which also drops the token
   kept for the old key, and for another app what was looked up for it. The
   app's login goes to `~/tehas/state/github/login`, so the first poll knows
   the bot. From here `gh` runs through `~/tehas/shim/gh`,
   which reads the app from these files and keeps the account of what the
   factory writes ([GitHub App identity](/github-app)).
3. It gives git the bot's login and noreply address
   (`<id>+<slug>[bot]@users.noreply.github.com`, the ID the one step 1 read),
   unless git already has a name or an email, which it keeps. git's credential
   helper for `github.com` is the app's `~/tehas/shim/gh`, given as
   environment-level configuration, which outranks git's files: nothing is
   written to them. That helper is what lets plain git read a private
   product's remote: the clone in step 4 and the fetches of
   [tehas dispatch](/operating), [tehas update](/operating) and
   [tehas status](/tools-status#tehas-status). A helper in git's files is never
   printed: it is a command an operator wrote and can hold a token. A git
   config file git cannot open stops the run with 78 here.
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
   server, runs `tehasbot github-token --check` (one mint, the login, the reach
   of `TEHAS_REPO`, never the token) and prints the dashboard's address.

A second run changes nothing that is right: a value that is there is kept, the
control issue is not made again, no second app is made and an image that is
there is not rebuilt. A
setting that is there and differs is never replaced without `--force`. The run
names each one and stops with 78 before it calls GitHub or the endpoint and
before it writes anything.

The whole run is the console's (`console setup run`), and what steps 1 and 2
say goes to standard error. It writes `models.json` with its keys sorted. A
release carries the console; from a checkout that has not built one, setup
builds it with cargo first, in the few lines of bash that are left of
`libexec/setup`.

Exit 0 when done, 1 when a check or a step failed (or the step's own status,
such as 78 from `install.sh`), 64 on an unknown option or app options that do
not go together, 78 for a setting that is missing, malformed or differs,
`--token-file`, a pasted address without the run's state, or when there is no
console and no checkout to build one from.

## tehas labels

```
tehas labels        # one line per label
```

Creates the twenty-two labels tehas uses in the product repository: `tehas:ready`,
`tehas:running`, `tehas:awaiting-merge`, `tehas:shipped`, `tehas:pause`,
`needs-human-attention`, `tehas:question`, `tehas:plan`, `tehas:refine`, `tehas:planning`,
`tehas:replaced` and `tehas:refined` (an issue a [refine](/operating#refining-a-request)
filed a ticket for), `tehas:millwright-escalated`, `tehas:blocked`, `tehas:refreshing` (on a
pull request a [refresh](/operating#refreshes) works on), `tehas:all-hands`,
`tehas:lean` (see [choosing the driver](/operating#choosing-the-driver)),
`epic` (see [epics](/operating#3-what-happens-next)), `tehas:finish` (see
[finishing a stopped cycle](/operating#finishing-a-stopped-cycle)),
`tehas:prototype` (see [the Protolab](/tools-protolab#tehasbot-protolab)), and
`tehas:improvement` and `cross-project` (the improvement loop's).
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
