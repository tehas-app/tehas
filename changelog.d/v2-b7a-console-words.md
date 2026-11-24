### Changes

- `tehas start`, `tehasbot preview`'s notice and `tehas init-product` say a failed GitHub call in tehas's words, one fixed line per kind of failure (`not found, or not visible to this actor`, `this actor may not do this`, `the budget is spent until later`, ...), after their own context. They no longer pass on gh's words or `HTTP <status>`: the notice's `no such pull request (HTTP 404)` and `GitHub refused the call (HTTP 403)` are now `not found, or not visible to this actor` and `this actor may not do this` (or `the budget is spent until later` for a rate limit), and `tehas init-product` no longer prints gh's standard error when the pull request is not opened.

### Adopter action

None. A script that matched gh's words or `HTTP <status>` in these lines matches the fixed lines.
