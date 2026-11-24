### Changes

- The dispatcher matches a login of `TEHAS_ALLOWED_LABELERS` ignoring ASCII case, as the console's checks do: a label applied by `Alice`, with the user id recorded for `alice`, now starts a cycle. 1.4.x matched exact case.
