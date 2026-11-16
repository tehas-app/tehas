# Security

Report a vulnerability privately, never in a public issue or pull request.
Use GitHub's private reporting: the Security tab of this repository, then
"Report a vulnerability".

Say what is affected, how to reproduce it, and the version (`tehas api`
prints it). We answer within a few days and fix in a release before the
report is made public.

tehas assumes its hosts sit on a private network with few users. The manual
page `host/docs/features.md` lists the known limits.

Releases are signed with minisign. The public key is
`host/release/minisign.pub`. The signing key is used only by the release
workflow, after a person approves the run.
