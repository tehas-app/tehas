### Changes

- A release now pins the pi-rukas commit its release gate built against. `scripts/tag-release.sh` names the commit before the gate and again after it, refuses to tag when upstream released in between, and writes the commit in the tag. The release workflow packs that commit, where it used to look the commit up again when it packed, after the approval.
- `tehas status`, the factory API and the dashboard name the pi-rukas release a pinned commit is (`versions.rukas.latest`), where a host installed from a release showed nothing.
