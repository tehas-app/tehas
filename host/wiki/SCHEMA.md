---
title: How this wiki is kept
verified_at: 00000000
---
This wiki is about one product, built from its code at one commit, its version
tags and its commit history. You keep it. The code is the truth: when a page
and the code disagree, the code wins and the page is changed.

## What you are given

- The product's code at the commit, read-only.
- `raw/head.json`: the commit, its date and `describe` (the latest tag and the
  commits since it). `raw/tags.json`: every version tag. `raw/log.jsonl`: one
  commit per line since the last build, or all of them on the first.
  `raw/changed.json`: the paths changed since the last build and the pages
  whose `sources` name one of them. `raw/changelog.md`, where there is one.
- This directory, the wiki as the last build left it. You change it in place.

## The pages

- `SCHEMA.md` is this file. `index.md` lists every other page, one line each
  with a link. `log.md` is the record of builds.
- Any other page is yours to make: an overview, the architecture, a page per
  part of the code, per concept, per decision, per release, a timeline, a
  glossary. A name is lower-case words and `-`, in up to three directories:
  `modules/dispatch.md`, `releases/v1-2-0.md`.
- Every page but `log.md` starts with front matter, and nothing else is in it:

  ```
  ---
  title: The dispatcher
  sources: [console/dispatch/]
  verified_at: <the first 8 hex of the commit>
  ---
  ```

  `sources` names the files or directories the page is about, so the next
  build knows which pages a change touched. `verified_at` is the commit you
  checked the page against: on every page you change, the commit of this
  build.

## The rules

- Link pages by relative Markdown links, `[the dispatcher](modules/dispatch.md)`.
  Every link lands on a page of the wiki or is `https://`. No HTML, no images.
- Every page is in `index.md`.
- Add one entry to the end of `log.md`, a `## ` heading with the date and the
  commit, then what you read and which pages you added, changed and removed.
  Never change an entry before it.
- Change what the new commits touched; leave the rest as it is. On a first
  build, write the overview and the pages a newcomer needs first.
- Name a commit by its hash. Never copy a secret, a token or a person's email.
- A page is at most 64 KiB.
