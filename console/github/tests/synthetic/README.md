# Synthetic answers

Hand-made stubs, not recordings. Each is a real recording under
`recorded/` with only the misbehaviour changed: GitHub's headers, ids and
shape are the recording's. Paths below are under `console/github/tests/`.

## anonymous/

Read by `tests/anonymous.rs` through `Stub::synthetic(<case>)`, before
`recorded/anonymous/`.

- `anonymous/no-location/repos_acme_widget_releases_latest`: a redirect
  with no place to go, which GitHub cannot be made to send. Source
  `recorded/anonymous/repos_acme_widget_releases_latest`; the status `200`
  changed to `302`. No `location` header, the release body kept.
- `anonymous/chain/repos_acme_widget_releases_latest`: the first of two
  redirects, which GitHub cannot be made to send. Source as above; the
  status changed to `302` and a `location:
  https://api.github.com/repositories/1/releases/latest` header added
  after it.
- `anonymous/chain/repositories_1_releases_latest`: the second redirect.
  Source as above; the status changed to `302` and a `location:
  https://api.github.com/repos/acme/widget/releases/latest` header added
  after it.
- `anonymous/busy/repos_acme_spent_releases_per_page_100`: a 429 with no
  reset time and no `retry-after`, which cannot be made on demand without
  spending a real budget. Source
  `recorded/anonymous/repos_acme_spent_releases_per_page_100` (a real 403
  limit); the status `403` changed to `429` and the `x-ratelimit-reset`
  line taken out.
- `anonymous/renamed/repos_acme_renamed_releases_latest`: to be replaced
  by the live recording, see the kit list (`e`, anonymous-renamed). Source
  `recorded/anonymous/repos_acme_widget_releases_latest`; the status
  changed to `301`, a `location:
  https://api.github.com/repositories/1/releases/latest` header added after
  it, and the body replaced by GitHub's documented redirect body
  (`message` "Moved Permanently", `url` the same address,
  `documentation_url`).
- `anonymous/renamed/repositories_1_releases_latest`: to be replaced by the
  live recording, as above. `recorded/anonymous/repos_acme_widget_releases_latest`
  as it is, at the address the redirect names.
