### Changes

- The review loop leaves core: `tehasbot review`, `tehas-review.service` and `tehas-review.timer`, and the store's `review-issue` and `review-file` verbs are gone. It returns as a plugin in 2.1. `tehas api --json` no longer has `review` in `flags`.
- `TEHAS_REVIEW` and `TEHAS_REVIEW_REPO` are no longer read. An instance that still sets one loads as before, with one warning line that names it, and the setting is dropped.
- `tehas update` and `install.sh` stop and remove `tehas-review.timer` and `tehas-review.service` where an earlier release left them.

### Adopter action

Take any `TEHAS_REVIEW` and `TEHAS_REVIEW_REPO` lines out of `instance.env`. Run `tehas update`: it removes the review units.
