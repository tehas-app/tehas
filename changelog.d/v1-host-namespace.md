### Changes

- The contract freezes the host namespace: `/host/` on the factory's origin belongs to the host that fronts the factory's domain, tehas serves nothing under it, and the host bridge stays `bridge: 1`. New tests hold the docs server's 404 for any path under `/host/`, a ticket note's link rule, and a host menu item opening in the dock.
