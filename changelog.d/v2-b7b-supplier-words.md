### Changes

- `tehas setup`, `tehasbot github-token` and the release's `console pins` say a failed call to GitHub, or to a site a pin is read from, in tehas's words, one fixed line per kind of failure (`not found, or not visible to this actor`, `the credential is no longer good`, `the site could not be reached`, ...), after their own context. They no longer pass on curl's words or `HTTP <status>`.

### Adopter action

None. A script that matched `HTTP <status>` or curl's words in the error lines of these commands matches the fixed lines.
