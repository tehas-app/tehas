EPIC: Epic

_Planned in pi-rukas plan session 5d1879e0-7be7-4544-a628-0cd6dfdc8a6c_

## Context & motivation

Descriptor: Epic: an outside bot asks a read-only agent about the product over MCP
Type: epic

## Prior context inventory

- # Epic: an outside bot asks a read-only agent about the product over MCP [context param]
- ## Why [context param]
- A chat bot connected to the MCP server can ask how the factory is doing, but not about the product: how a part of the code works, where something is, what a change would take. It also cannot turn a conversation into a well-formed issue. Both need an agent that reads the code, and pi-rukas's planner already is one. [context param]
- ## What [context param]
- Three MCP tools, backed by a runner on the host: [context param]
- | Tool | Does | [context param]
- | --- | --- | [context param]
- | `project_ask` | Question, optional session id. Queues a turn of a read-only agent over the product at the default branch. Returns a job id at once | [context param]
- | `project_answer` | Job id. Returns the status, then the answer text and the session id for a follow-up | [context param]
- | `project_plan` | Same, with pi-rukas's planner. Returns its questions or the drafted plan. A further call with `file: true` files the drafted plan as an issue | [context param]
- Answers take minutes, so the flow is async. The bot polls `project_answer`, or waits for a callback: [context param]
- - A key may have a callback URL, set on the host beside the key, never in a request. A URL from a request would let a key holder make the host call any address. [context param]
- - When a job ends, the runner POSTs `{"job", "status"}` to it, signed with an HMAC of the body under a secret for that key. The body has no answer text: the bot fetches it with `project_answer`, under its key. [context param]
- - One try and one retry, then nothing. Polling always works, so a lost callback loses no answer. [context param]
- - The runner makes the call, not the MCP server. [context param]
- ## Skills [context param]
- A skill makes a common question repeatable: the same question gets the same kind of answer, from the same data, every time. [context param]
- | Skill | Asks | Data | [context param]
- | --- | --- | --- | [context param]
- | `working-on` | What are you working on? | Running cycles, the line | [context param]
- | `issue` | How far is ticket 376? | One issue's story: stage, attempts, park reasons, PR | [context param]
