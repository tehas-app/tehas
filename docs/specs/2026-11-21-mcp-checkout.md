# The MCP server reads the shared checkout

Issue #2709, step 1 of epic #2689. An outside agent asks why the product is
the way it is. The git history is the first source: the log, a commit and its
diff, and who last changed each line.

## What it is

- `TEHAS_MCP_CHECKOUT`, a host setting: the path of one clone of the product
  with its whole history. Every MCP session and every agent user on the host
  reads the same clone. Unset, nothing changes.
- `tehas mcp` mounts it read only at `/checkout` and passes
  `--checkout /checkout`. A path that is no directory stops it with 78.
- The server opens it with gix at start and on every call. Not a git
  repository with a commit, or a shallow clone: 78.
- Three tools, full keys only: `factory_git_log`, `factory_git_show`,
  `factory_git_blame`. Listed beside the plugin tools, outside the catalog of
  sixteen, as the plugin tools are.
- Every answer carries `head` and `untrusted`, and text is masked as logs are.
  An author is named, never an address.

## No program runs

gix reads objects; it runs nothing unless asked. The server opens the clone
with `Options::isolated()`: no system, user or environment config. Diffs and
blame read blobs as git stores them (`Mode::ToGit`), so no textconv, filter,
external diff or fsmonitor is run whatever the clone's config and attributes
say. A test plants each of them in a clone's config, shows that `git` itself
runs them, and shows that the server does not.

A commit argument is hex only, so no revision syntax reaches gix. A path is
relative, with no `.` or `..` part.

## Changes to the MCP server

A security change. `host/docs/mcp-server.md` changes with the code.

| Where | Was | Is |
| --- | --- | --- |
| What it reads | `~/tehas/live`, `~/tehas/mcp`, `~/tehas/logs` | The same, and the shared checkout when `TEHAS_MCP_CHECKOUT` is set |
| What it runs | Nothing | Unchanged: the checkout is read in the process, no `git` program |
| The container's mounts | The four fixed, then the plugin calls (#2692) | The same, then `<TEHAS_MCP_CHECKOUT>:/checkout:ro`, only when it is set. Pinned by `test/fixtures/argv/mcp-checkout` |
| Its arguments | ... `--plugin-calls <socket>` | Then `--checkout /checkout`, before `--outsource` |
| Its network | `tehas-mcp`, no masquerade | Unchanged |
| A full key | The factory's logs and sessions | And the product's code and history |
| A dashboard key | The four live-file tools | Unchanged |

## Cost

gix with `blame`, `revision` and `sha1` and no default features adds 107
crates to the lock file and doubles the release binary, from 3.4 to 7.3 MiB.

## Not in this step

Fetching the clone on a new connection and telling sessions about it (step 2).
The 2026-07-28 revision the server speaks keeps no session, so step 2 has to
find another way than a session id to tell a client the clone moved.
Tickets, the code index and learnings are steps 3 to 5.
