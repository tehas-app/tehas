### Changes

- A planning or refine session's label changes are one GitHub request per label: those that go on first, then those that come off. A poll stopped between them leaves the issue more held, never unflagged. A part that fails, or that may have been posted, is said in `logs/dispatch.log`.
- The plan written onto its issue keeps the issue's markers. Before, writing the plan replaced them with the plan's text, so a restraint on the issue was lost. Now it survives the plan edit.
