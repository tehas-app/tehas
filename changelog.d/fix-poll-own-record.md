### Changes

- The poll decides a cycle's end from the host's own record. A push or a pull
  request that only the cycle's result says failed is on record and in
  `tehas status`, but the poll no longer finishes it on its own: it waits for
  `tehas:finish`.
- A cycle whose result names a provider outage is parked under its cap and
  waits for a person, with a retry box. It is no longer started again on its
  own. A park recorded as `provider-outage` by an earlier version keeps its
  one retry.
- A refused `tehas:finish` tells the issue the rule's own words. What git or
  gh said goes to `dispatch.log` on a line of its own.
