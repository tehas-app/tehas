Epic: an outside bot asks a read-only agent about the product over MCP

## Why

A chat bot connected to the MCP server can ask how the factory is doing, but not about the product: how a part of the code works, where something is, what a change would take. It also cannot turn a conversation into a well-formed issue. Both need an agent that reads the code, and pi-rukas's planner already is one.

## What

Three MCP tools, backed by a runner on the host:

| Tool | Does |
| --- | --- |
| `project_ask` | Question, optional session id. Queues a turn of a read-only agent over the product at the default branch. Returns a job id at once |
| `project_answer` | Job id. Returns the status, then the answer text and the session id for a follow-up |
| `project_plan` | Same, with pi-rukas's planner. Returns its questions or the drafted plan. A further call with `file: true` files the drafted plan as an issue |

Answers take minutes, so the flow is async. The bot polls `project_answer`, or waits for a callback:

- A key may have a callback URL, set on the host beside the key, never in a request. A URL from a request would let a key holder make the host call any address.
- When a job ends, the runner POSTs `{"job", "status"}` to it, signed with an HMAC of the body under a secret for that key. The body has no answer text: the bot fetches it with `project_answer`, under its key.
- One try and one retry, then nothing. Polling always works, so a lost callback loses no answer.
- The runner makes the call, not the MCP server.

## Skills

A skill makes a common question repeatable: the same question gets the same kind of answer, from the same data, every time.

| Skill | Asks | Data |
| --- | --- | --- |
| `working-on` | What are you working on? | Running cycles, the line |
| `issue` | How far is ticket 376? | One issue's story: stage, attempts, park reasons, PR |
| `next` | What is next in line? | The line, in order |
| `ended` | What ended this week? | Endings of the last 7 days |
| `failures` | What failed, and why? | Failures and park reasons |

`project_skills` lists the skills with their names, descriptions and arguments. `project_ask` takes `skill` and `args` instead of free text, for example `{"skill": "issue", "args": {"number": 376}}`.

There are two kinds:

- **Data skills** answer from factory data alone, with no model. They are fast, cost nothing, and give the same answer to the same state. All five above are data skills.
- **Agent skills** are a prompt for the read-only agent, with the data it should be given. For example, `explain-issue`: what issue N asks for and which code it touches.

### Adding skills

Skills are files, one per skill: built-in ones in the release, and the instance's own in `<instance>/skills/`. A skill in the instance with the name of a built-in replaces it. Adding one is a commit to the instance repository and a pull on the host. No release is needed.

A skill file is Markdown with frontmatter:

```markdown
---
name: issue
description: How far one issue is
kind: data            # data | agent
args:
  number: {type: integer, required: true}
data: [issue]         # what the skill is given, from a fixed list
---
Issue {{number}}: {{issue.title}}
Stage: {{issue.stage}} ...
```

For a data skill the body is a template filled from the named data. For an agent skill it is the prompt.

- `data` names only sources from a fixed list in the code: `status`, `line`, `issue`, `endings`, `failures`, `sessions`. These are the data the MCP tools already serve, masked as they serve it. A skill cannot name a file or a command, so a skill file cannot widen what a key sees.
- An agent skill's data is written into the job as a read-only file. The container gets no MCP access and no new mount.
- A skill file that does not parse, names an unknown source or has a bad argument is refused when the runner loads the skills, with one line that names the file. The other skills still load.

## Boundary

The agent may read the code and file issues. It may not edit the code or submit work. The environment enforces this, not the prompt:

- The checkout is a throwaway clone at a pinned commit, mounted read-only, removed after the job.
- The container gets no git credentials and at most a token that can only read issues, which the planner's duplicate check needs. It cannot push, open a PR or comment.
- The read-only mount is the real wall. pi-rukas leaves the write and edit tools out of its explore agents, but their bash can still write files (pi-rukas `docs/headless.md`, the child table).
- Filing happens on the host, after the container exits, from the drafted plan's title and body: the runner files it with a token limited to issues. The container never holds that token.
- A filed issue is unlabelled. It enters the line only when an allowed account applies the label (AGENTS.md rule 5).
- The container gets the sandbox limits of `tehas plan`, and no more.
- The runner, not the MCP server, starts containers. The server only queues jobs and reads results, so the limits of epic #835 hold for it: no GitHub, Docker, git or model call from the server.

## Network

The first client's host and the factory's host are on the same private network (a tailnet). The bot can reach the MCP port directly, without the public `/mcp` route, and the callback can go to a private address.

- Being on the network does not give access. Every call still needs a key, and every callback is still signed. A bad or missing setting stops the server with exit 78, as it does now.
- Callback URLs stay a per-key host setting. A setting may limit them to the private network's address range, and a URL outside the range is refused when the runner loads the settings.
- The job container gets no new network access. Only the runner on the host makes callbacks.
- Serving the tools on the public route is the same code with the same keys. Nothing here depends on the private network.

## Capacity

Agent jobs (`ask`, `plan`) run in a lane of their own, apart from cycles. They never wait for a cycle slot and never take one.

- The lane has its own queue and its own concurrency limit, an instance setting. 0 switches agent jobs off.
- Each job container gets its own memory and pids limits, outside the cycle slice. The host must fit the slice and the lane together, and the manual says how to size both.
- Data skills are not jobs. They use no model, and the MCP server answers them at once.
- Per key: a limit on queued jobs, on issues filed per day, and on the time of one turn. All are instance settings.
- The model endpoint is shared with cycles. More parallel questions mean slower cycles there; the lane's limit is how an operator trades one for the other.

## Keys

A key holder can read the whole repository through the agent and file issues on it. `host/docs/mcp-server.md` says so where it explains who should get a key.

## Parts

In work order. Each part changes `host/docs/mcp-server.md` and `host/docs/features.md` in its own commit.

1. **Data skills on the MCP server.** The skill loader (built-in skills from the release, the instance's own from `<instance>/skills/`, a bad file refused alone), `project_skills`, and `project_ask` with `skill` and `args` for data skills, answered at once from the data the server already reads. The five built-in data skills. No queue, no runner, no model. The server's container gets no new mount: the skills reach it as a file another program writes into `~/tehas/mcp/`, as the sessions snapshot does (epic #835's limits).
2. **The job queue and the lane.** Jobs in the state store, the lane's concurrency limit and the per-key limits as instance settings, `project_ask` (free text and agent skills) queuing a job, `project_answer` reading it. The server writes a job and reads results; it starts nothing.
3. **The runner unit.** Takes jobs from the queue within the lane's limit and runs each in the sandbox image on a throwaway read-only clone at the default branch, with its own memory and pids limits, no git credentials and no token that can write. Tests pin the container's mounts and environment. Runs `plan` turns first, since they need nothing new from pi-rukas.
4. **`project_plan` and filing.** Planner turns through the lane, with the session kept out of the checkout by `PI_ENSEMBLE_PLAN_SESSION_DIR`. `file: true` files the drafted plan on the host, unlabelled, with a token limited to issues and the per-key daily limit.
5. **Free-text `ask`.** Runs `pi-rukas-headless ask` once the image lists `verbs.ask` (trail-openers/pi-rukas#1896); before that `project_ask` without a skill says it is not available. Agent skills run here, with their data written into the job as a read-only file.
6. **Callbacks.** Per-key callback URL and HMAC secret on the host, optionally limited to an address range, and the runner's signed POST of `{"job", "status"}` with one retry.

## Out of scope

- Any tool that edits code, opens a PR, labels or comments.
- Giving a cycle's container MCP access (AGENTS.md rule 4).

## Done when

- A bot with a key asks a question about the code over MCP and gets an answer from an agent that read it.
- The same bot drafts and files an issue through `project_plan`, and the issue is unlabelled.
- `{"skill": "issue", "args": {"number": 376}}` answers how far issue 376 is, with no model call.
- A skill added in `<instance>/skills/` shows in `project_skills` after a pull, with no release. A bad skill file is refused alone.
- A bot with a callback URL gets a signed POST when its job ends, and one without it gets the answer by polling.
- Tests show the job container gets no GitHub token that can write, no git credentials and a read-only checkout.
