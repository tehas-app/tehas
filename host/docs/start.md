---
title: Start
description: "Tell your coding agent to read this page. It finds what your repository needs, installs tehas and onboards the repository, and stops only where a person has to decide."
---

# Start

Tell your coding agent one of these:

```
Go to https://tehas.app/start and set up tehas for this repository.
```

```
Go to https://tehas.app/start and find what this repository needs from a tehas factory.
```

The rest of this page is for the agent. A person reads it too: it says
where you will be asked something. Nothing else needs you.

## For the person: what you will be asked

The agent does the work. It stops and asks you when something is yours to
decide:

1. **A machine.** One Linux host for the factory: Ubuntu 26.04 on x86_64,
   and root on it once. The agent tells you how big, from what your
   repository needs. A laptop is not the place: a factory runs all the
   time.
2. **A model endpoint.** The address of an OpenAI-compatible endpoint the
   host can reach, with its `/v1`, and its key if it takes one.
3. **The factory's GitHub App.** You open one link, check what the app may
   do and create it, then install it on the repository. Two steps in your
   browser, signed in to GitHub. tehas keeps the app's key itself.
4. **Who may start work.** The GitHub logins whose `tehas:ready` label
   spends compute. Never the bot.
5. **The onboarding pull request.** The bot opens one pull request with
   the files your repository lacks: `AGENTS.md` if there is none, and the
   gate it guessed for your stack. You read it and merge it.

Then you label one small issue `tehas:ready` and watch it on the
dashboard. Nothing merges on its own.

## For the agent

### Rules

- Ask your person before anything about secrets, accounts, the machine or
  who may authorize work. Never invent a key, an endpoint or a login.
- Never put a key in a commit, an issue, a comment or your own notes. On
  the host, keys live in `~/.config/tehas` and nowhere else.
- Run nothing as root but the installer, and show your person the command
  first.
- A step that fails stops you. Report its output; do not work around it.
- Be honest about what is built: [Features](/features) says how far each
  part is proven.

The whole manual is at [`/llms.txt`](/llms.txt) as links and at
[`/llms-full.txt`](/llms-full.txt) as one text.

### 1. Find what the repository needs

Do this first, in the repository, on the machine you are on now. No host is
needed yet. You are the wizard: read the repository, work out what it
needs, and ask your person only what the files cannot tell you.

1. Read `AGENTS.md`, the manifests and lockfiles (`package.json`,
   `Cargo.toml`, `go.mod`, `pyproject.toml` and the like), the version
   files (`.tool-versions`, `.nvmrc`, `rust-toolchain.toml`),
   `docker-compose*.yml`, `.devcontainer/` and `.github/workflows/`.
2. Find the commands that prove a change: a fast gate, a full gate, and a
   smoke test that the built product starts. Run each once in a clean
   checkout. A gate you could not run is not a gate yet: say so.
3. Note what the factory's sandbox image will need beyond Node, Rust and
   the platform's tools: other languages and their versions, system
   packages, the hosts the install downloads from.
4. Note what the gates reach beside the code. A cycle gets no database or
   other service of its own: a gate that needs one has to start it inside
   the cycle, or the product is not ready for a factory yet.
5. Tell your person what you found, what you guessed and what you could
   not tell, and the size of machine that fits. A factory runs as many
   cycles at once as the host has CPUs minus one, each with its own share
   of memory; the example host of this manual has 8 CPUs and 15 GB.

The files tehas reads in the product are the contract on
[A new project](/new-project#what-the-product-repository-must-provide).
Put what you found there: the gates in `.pi/verify-cmd` and its siblings,
on a branch your person merges.

### 2. Install a factory

Read [A new project](/new-project) in full. It is the install guide;
this is the order and where to stop.

1. **Check the machine.** On the host: `cat /etc/os-release` and
   `uname -m`. Anything other than Ubuntu 26.04 on x86_64 stops here: tell
   your person that tehas does not run there yet.
2. **Install the release**, as root, naming the Unix user the factory runs
   as (`boss` here). Ask your person to run it, or to let you:

   ```
   curl -fsSL https://github.com/tehas-app/tehas/releases/latest/download/install | sudo sh -s -- boss
   ```

   The installer checks the download's checksum and signature before it
   unpacks anything. [Bootstrap a host](/bootstrap) says what it
   installs.
3. **Ask your person** for the model endpoint and its key, and the
   labelers' logins.
4. **Run setup** as the tehas user:

   ```
   ~/tehas/current/bin/tehas setup --repo <owner>/<repo> --labelers "<logins>" \
     --new-app --endpoint <endpoint>/v1
   ```

   It prints one link. Give it to your person, who creates the app and
   pastes back the address GitHub opens. Setup then waits until the app is
   installed, checks the endpoint, writes the settings, clones the product,
   builds the sandbox image and proves the product's gates in it. A second
   run changes nothing that is right. See
   [tehas setup](/tools-setup#tehas-setup).
5. **Onboard the repository:**

   ```
   tehas check-product
   tehas init-product
   ```

   [tehas check-product](/tools-setup#tehas-check-product) lists what the
   repository lacks. [tehas init-product](/tools-setup#tehas-init-product)
   opens a pull request with the missing `AGENTS.md`, `.pi/verify-cmd` and
   `.pi/worktree-setup`, guessed from the stack. Compare its guesses with
   what you found in step 1 and tell your person where they differ. Once it
   is merged, run setup again so it proves the gates.
6. **Put the toolchain in the image.** What the product needs beyond the
   platform's image goes in `sandbox.Dockerfile` in the instance directory,
   as [A new project](/new-project#5-the-sandbox-image) says. Run setup
   again; it rebuilds the image and proves the gates.
7. **Hand over.** Give your person the dashboard's address that setup
   printed, and one issue you wrote that a cycle can finish: small, one
   change, with how to check it. Your person labels it `tehas:ready`.
   [Operating](/operating) takes it from there.

### 3. Add a feature beside the factory

Plugins are not built yet. Their design is a draft in the repository,
[docs/specs/2026-11-21-plugins.md](https://github.com/tehas-app/tehas/blob/main/docs/specs/2026-11-21-plugins.md):
a program from a git repository pinned to a commit, run in a locked-down
container outside the cycle, given nothing until a person grants each
thing it asks for. When a release ships them, this page says how to write
one.

Until then, a program beside the factory can use what tehas-industry uses:
the factory's commands with `--json` ([The factory API](/api)), the
read-only [MCP server](/mcp-server), and up to four menu items of its
own pages with `tehas panel set` ([Panels](/api#tehas-panel)). It reads
the factory and shows its own pages. It never writes to the product or to
the factory's state.

## What is not there yet

- Only Ubuntu 26.04 on x86_64.
- One repository per install. A second needs its own Unix user, or better
  its own machine.
- No database or other service per cycle.
- No hosted trial: a factory always runs on a machine of yours.
- Plugins.
