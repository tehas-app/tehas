### Changes

- Plugins, still being built: the hook after work. A cycle that ends with an open pull request of the bot's, and was not stopped or refused at its start, asks each plugin granted `hook:after:work` and ready for a hook on it, and waits for none. When the hook ends, tehas posts one commit status, `tehas/plugin/<name>`, on the commit it read: `success` for a `pass` or `note` verdict, `error` when the hook did not finish, the verdict's summary cleaned and cut at 140 characters, and a link to the plugin's findings page with `view` granted. Never a comment. A head that moved while the hook ran is asked for again. `tehas plugin` says when GitHub refused the status, and `tehas plugin --json` gains `status_unwritten`. `tehas setup --new-app` now asks for "Commit statuses: Read and write".

### Adopter action

Before you turn plugins on with a hook granted, give the factory's GitHub App "Commit statuses: Read and write" (the App's settings, Permissions and events, Repository permissions), then accept the new permission on the installation. Without it the hook still runs, but its status is not written and `tehas plugin` says so. A container's narrowed token is unchanged.
