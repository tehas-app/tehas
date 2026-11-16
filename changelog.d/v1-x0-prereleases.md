### Changes

- The release workflow also publishes signed prereleases: a tag `vX.Y.Z-alpha.N`, `vX.Y.Z-beta.N` or `vX.Y.Z-rc.N` is built, signed and published as a GitHub prerelease, never as the latest release. `scripts/tag-release.sh --pre alpha|beta|rc` tags the next one.
- `tehas update` reads alphas and betas beside release candidates and orders them alpha, beta, rc, then the release. A plain update passes over every prerelease; `tehas update --to <tag>` installs one by name.
- `tehas status --json` names the commit the installed release was built from in `versions.platform.commit`, and the health file's `checks.platform` carries it.
