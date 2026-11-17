### Changes

- `tehas update` reads releases from the repository built into tehas, or the
  one `TEHAS_RELEASE_REPO` in `instance.env` names, and checks their signature
  with the key built into tehas. It no longer reads the repository from
  `~/tehas/versions.env` or the key from `~/tehas/release/minisign.pub`, both
  of which a release writes. An instance that names another repository with
  `TEHAS_RELEASE_REPO` still checks with the built-in key.
- `tehas update` asks the console whether it can run the update by its exit
  status as well as its answer, so a console that answers and fails is not
  asked to update.
- A shell test runs the update on a home a v0.30.x install left
  (`test/update_from_030_test.sh`). [Update](/tools-update) says
  how a v0.30.x host moves to 1.0.

### Adopter action

An instance that sets `TEHAS_RELEASE_REPO` to a repository whose releases are
signed with another key can no longer update from it: take the line out to
update from tehas's own releases.
