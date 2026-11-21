### Changes

- Plugins, still being built: `install.sh` builds the gate `tehas-plugin-gate` and installs its unit `tehas-plugin-gate.service` (`tehasbot plugin-gate`) only while the feature is on. The edge router sends every `plugin-<name>` and `plugins` name to the gate before anything else, at `TEHAS_PLUGIN_GATE_ADDR`, and answers a name no plugin has and a gate that is down with texts of their own (#2633).
- `tehas setup --new-app` names the callback URL `https://plugins.<domain>/_auth/callback` in the app's manifest when `instance.env` has `TEHAS_DOMAIN`, and keeps the client id and client secret GitHub returns in `~/.config/tehas/github-app.client-id` and `github-app.client-secret`, mode 600. `tehas setup --app-client-id <id> --app-secret-file <file>` writes them for an app made before (#2633).
- `TEHAS_PLUGIN_GATE_ADDR` is `TEHAS_HOST_ADDR`'s address on port 8095 when the instance names none, where the router reaches it, in place of `127.0.0.1:8095`. An IPv6 address in it is in brackets, as `[::1]:8095`; the unbracketed form stops every command with 78 (#2633).
- `tehas update` docks a ready plugin's view as the panel `p-<name>` and takes it away when the plugin goes or is refused. `tehas panel set` refuses an id that starts `p-` with 64 (#2633).

### Adopter action

Only for a factory that will run plugins once they ship. For a GitHub App made before this release: on the app's settings page add the callback URL `https://plugins.<domain>/_auth/callback` (`plugins-<sub>.<domain>` with `TEHAS_EDGE_ONE_LEVEL=on`), generate a client secret, copy it into a file, and run `tehas setup --app-client-id <Client ID> --app-secret-file <file>`. In the proxy in front of the host, add `plugin-[a-z][a-z0-9]*` and `plugins` to the matcher of the edge's names, and in Cloudflare add a cache rule that bypasses the cache for both names. See the manual's pages on the edge ("Plugins' views") and on the GitHub App.
