### Changes

- The approval record no longer carries `comment_links`: the state store refuses the key on a new approval, and an approval stored before 2.0 that holds it loads with the key ignored. `attach issue` no longer takes `--comments`.
