### Changes

- A gh with no credential for GitHub (gh's exit 4, "authentication required") is now told as "the credential is no longer good", where it was "the site failed; it may have been sent". gh asks nothing in that case, so nothing is read again for it: the poll says a write it could not make once, in place of a warning that the next poll reads it, and a cycle's end fault names the factory's own credential, not GitHub.

### Adopter action

None. A program that reads the poll's or a cycle's lines for "may have been sent" no longer sees it for a gh that is signed out.
