### Changes

- A label pins file (`state/labels`) that is there but cannot be read, such as a symbolic link, a file tehas may not read or one with a malformed line, no longer counts as no file. The poll refuses every granting `tehas:` label and logs one warning line until a person repairs it, and `tehas labels` stops with exit 78. Neither writes over the file. See the label pins section of the GitHub App page.
