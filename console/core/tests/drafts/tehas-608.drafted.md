feat: Plan issue #608

_Planned in pi-rukas plan session 4ceeead2-2eb2-43e4-b80c-d577154084dc_

## Context & motivation

Descriptor: Plan issue #608: TEHAS_READ_ONLY: an instance that writes nothing to GitHub
Type: feature

## Prior context inventory

- # TEHAS_READ_ONLY: an instance that writes nothing to GitHub [context param]
- ## What [context param]
- A setting, `TEHAS_READ_ONLY=1`, under which an instance reads its product's issues and code and runs cycles as usual, but writes nothing to GitHub. [context param]
- ## Why [context param]
- tehas-industry is gaining a dev mode: a developer runs an industry and one factory on their own laptop, for working on tehas, industry or a product's setup. A factory on a laptop is still a real factory. Pointed at a repository that a shared factory also serves, it would pick up the same `tehas:ready` issues, push branches and open duplicate pull requests, and a laptop that sleeps in the middle of a cycle leaves labels behind. Dev factories should be read-only unless someone chooses otherwise. [context param]
- Industry will also check the token's role on the product repository and refuse anything above read for a read-only factory. That check holds even if a write slips through, but on its own it turns every write into a failure in the middle of a cycle. The instance should know it is read-only. [context param]
- ## Proposed behaviour [context param]
- - Every GitHub write goes through one guard: labels, comments, reactions, pushed branches, pull requests, issue edits. Under `TEHAS_READ_ONLY=1` the guard refuses each one and logs what it would have done. Nothing else decides it. [context param]
- - The queue state that labels hold today (`tehas:running` and the rest) is kept locally, so the dispatcher still knows what it started. [context param]
- - A cycle's result is its branch in the workspace and the pull request text it would have opened. Both are shown on the dashboard. [context param]
- - `tehas status --json` says the instance is read-only, so industry can confirm it before it starts the factory. [context param]
- - Off by default. Instances that do not set it are unchanged. [context param]
- ## For industry [context param]
- Industry refuses to create a read-only factory at a tehas tag that does not know the setting, so it needs to know the first release that has it: a line in `CHANGELOG.md` is enough. [context param]
- --- [context param]
- ## How to plan this: tehas refine [context param]
- This request was labelled `tehas:refine`. Plan it into one issue that a single [context param]
- unattended `/work` cycle finishes. The cycle has these limits: explore 30 [context param]
- minutes, develop 90 minutes, and a review loop of at most a few hours (six [context param]
- review lenses, one fix round, an adversarial re-check). A cycle that cannot [context param]
- finish in that leaves a half-reviewed draft pull request. The request above may [context param]
