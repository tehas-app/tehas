### Changes

- The state store moves to schema 22 at the next `tehas update`, which backs it up first: a plugin's record keeps the GitHub user id of the person who filed its issue, beside the login.
- A cycle is given the files a plugin's record names when a person who may label filed them in its issue: a `tehas-files:` line of the approved body, checked by GitHub id and by the files' sha256. Files that changed after filing stop the start with `plugin-files-changed`. No plugin files records yet.
