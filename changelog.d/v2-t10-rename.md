### Changes

- The repository is `tehas-app/tehas-factory`. Only its name changes: the commands, paths and settings keep theirs. Releases, the installer's address and the `$id` of every schema in `api/` name it. The old name, `tehas-app/tehas`, still works while GitHub redirects it, and `tehas update` says so when `TEHAS_RELEASE_REPO` names it.

### Adopter action

Remove `TEHAS_RELEASE_REPO=tehas-app/tehas` from `instance.env` or name `tehas-app/tehas-factory`. Point clones, links and a pinned schema `$id` at the new name: the old one stops redirecting when it is reused.
