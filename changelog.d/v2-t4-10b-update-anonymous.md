### Changes

- `tehas update` no longer asks `gh auth status` which way to read the releases. It asks with gh, and reads with no credential only when gh's answer says it cannot: gh has no credential, its credential is refused, it may not read the repository, or there is no gh to run. A failure of GitHub's, or one that may have reached it, fails the update with its reason and no longer turns to the public address.
- With no credential, the list and the download go through the code host, as the signed-in ones do. The list reads every page of releases, not only the newest 100. Each file is read by its id from GitHub's API, following at most two redirects to GitHub's own file hosts, over https alone. A file over 1 GiB is refused before it is fetched, and one served at another size than listed is refused. A failure says `could not list the releases of <repository>: <reason>` or `could not download <file> of <repository>: <reason>`. A private repository adds "(a private repository needs gh signed in)", as before.
- The anonymous reads keep GitHub's rate budget in `state/ledger.json` of the home.

### Adopter action

None. A host whose gh is signed out keeps updating from a public repository. A host whose gh fails on GitHub's side no longer falls back to the public address: run the update again.
