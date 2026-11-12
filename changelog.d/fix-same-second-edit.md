### Changes

- An edit in the second of an approval ends it. GitHub's times count whole
  seconds, so an edit or rename of an issue by another account in the second
  `tehas:ready` was applied, or a retry was ticked, now counts as after it, and
  the issue waits until it is approved again. The approver's own change in that
  second is still the text it approved.
