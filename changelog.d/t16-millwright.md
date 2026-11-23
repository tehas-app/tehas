### Changes

- The millwright's old name, the fixer, is gone from what tehas reads and writes. The state store's schema 28 drops `fixer` from what a cycle's `how` may say and `fixer-refused` from the planning notes, and writes `millwright` and `millwright-refused` in any row that still said them. `store approval-record --how fixer`, `store history-show --how fixer` and `store plan-note --kind fixer-refused` are refused; use `millwright` and `millwright-refused`.
- The approval record a cycle is given says `how: "millwright"` for a millwright's start, and a cycle no longer takes one that says `fixer`.
- `store plan-view` marks a millwright's answer `millwright` and lists the refused replies under `millwright_refused`, in place of `fixer` and `fixer_refused`.
- Analytics rewrites a run recorded as `fixer` to `millwright` in `runs.how` of `state/analytics.db` when it opens the file, and marks a planning turn the millwright's only when its answer says `millwright`.
- The dispatcher's log and its comments on an issue name the millwright, not the fixer.

### Adopter action

None. The state store migrates to schema 28 at the next `tehas update`. A program that reads the store's approval or planning views, or filters runs by `how`, reads `millwright` and `millwright_refused`.
