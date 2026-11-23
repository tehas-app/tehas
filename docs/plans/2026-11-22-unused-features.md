# Features that could leave core

**Status**: decided by the owner, 2026-11-22. Every row is approved, and
T16 of `docs/plans/2026-11-21-coding-standard-v2.md` carries them out.

tehas 2.0.0 removes the Protolab from core and parks it (T14 of
`docs/plans/2026-11-21-coding-standard-v2.md`, recorded in
`docs/specs/parked-protolab.md`), and its job moves to the lab plugin
(`docs/specs/2026-11-22-plugin-lab.md`). The rows below are the other
features that meet the same test: built, off on every factory, never or
barely seen on a host (`host/docs/features.md`).

"Plugin" means it moves to a plugin after 2.0, with its plumbing kept. A
plugin that needs a surface the specification lacks comes with a revision
of its own. "Drop" means it goes in 2.0.

| Feature | Evidence | Proposal | Decided |
| --- | --- | --- | --- |
| Protolab | Envisioned, never pulled through | Drop from core, parked with a record (`docs/specs/parked-protolab.md`); its job goes to the lab plugin | Owner, 2026-11-22 |
| All hands label | Never on a host. It only shows in status | Drop || Drop in 2.0: Owner, 2026-11-22 |
| Spec mark (`Spec:` lines) | Tests only. Its one filer was `tehas prototype file`; the page ticket hands files through records instead | Drop || Drop in 2.0: Owner, 2026-11-22 |
| Panels (outside pages in the dock) | A development machine only | Plugin: a view in a frame || Plugin in 2.1: Owner, 2026-11-22 |
| Business page | A development machine only | Plugin, over the live feeds || Plugin in 2.1: Owner, 2026-11-22 |
| Energy use | Tested with fake files | Plugin: a panel and a timed fetch. Check the host's private settings first || Plugin in 2.1: Owner, 2026-11-22 |
| Host panel (`TEHAS_TOP`) | Tests only | Plugin. `tehas top` stays as an operator command || Plugin in 2.1; `tehas top` stays: Owner, 2026-11-22 |
| Review loop (`tehasbot review`) | Never on a host | Plugin: a timed run with the model that files an issue. Needs `issue:draft` and a timed run || Plugin in 2.1: Owner, 2026-11-22 |
| Product wiki | Being built, step 2 in review | Plugin: a timed run with the model over the checkout, and a served page || Plugin in 2.1: Owner, 2026-11-22 |
| Describe, text files, zips and comments for a cycle | No host has run them. Images stay in core | Plugin: a hook before a cycle starts that adds files. Needs a `before:work` hook || Plugin in 2.1; images stay: Owner, 2026-11-22 |
| Outsource, with the model proxy and the contractor | Being built. A release refuses it | Owner's call. About 20k lines across 11 crates, too deep for a plugin || Drop in 2.0: Owner, 2026-11-22 |
| Millwright's grant | Never on a host | Owner's call. It is an authorisation path, so it stays in core or goes || Drop in 2.0: Owner, 2026-11-22 |

Kept in core, though not proven on a host: refreshes, read-only instances,
the egress limit, the lean driver, analytics and the operator commands.
They are part of the loop, the sandbox or how a person runs a factory.

A drop in 2.0 is a step on the v2 branch and a line in T9's "Changed in
2.0", with its readers named.
