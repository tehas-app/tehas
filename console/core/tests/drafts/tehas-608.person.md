TEHAS_READ_ONLY: an instance that writes nothing to GitHub

## What

A setting, `TEHAS_READ_ONLY=1`, under which an instance reads its product's issues and code and runs cycles as usual, but writes nothing to GitHub.

## Why

tehas-industry is gaining a dev mode: a developer runs an industry and one factory on their own laptop, for working on tehas, industry or a product's setup. A factory on a laptop is still a real factory. Pointed at a repository that a shared factory also serves, it would pick up the same `tehas:ready` issues, push branches and open duplicate pull requests, and a laptop that sleeps in the middle of a cycle leaves labels behind. Dev factories should be read-only unless someone chooses otherwise.

Industry will also check the token's role on the product repository and refuse anything above read for a read-only factory. That check holds even if a write slips through, but on its own it turns every write into a failure in the middle of a cycle. The instance should know it is read-only.

## Proposed behaviour

- Every GitHub write goes through one guard: labels, comments, reactions, pushed branches, pull requests, issue edits. Under `TEHAS_READ_ONLY=1` the guard refuses each one and logs what it would have done. Nothing else decides it.
- The queue state that labels hold today (`tehas:running` and the rest) is kept locally, so the dispatcher still knows what it started.
- A cycle's result is its branch in the workspace and the pull request text it would have opened. Both are shown on the dashboard.
- `tehas status --json` says the instance is read-only, so industry can confirm it before it starts the factory.
- Off by default. Instances that do not set it are unchanged.

## For industry

Industry refuses to create a read-only factory at a tehas tag that does not know the setting, so it needs to know the first release that has it: a line in `CHANGELOG.md` is enough.
