### Changes

- The docs server no longer serves the live console of a running cycle unless the instance sets `TEHAS_SITE_CONSOLE=on`. Without it `/live/console-<n>.txt` and its `.now` file answer 404 and the console panels of the site stay empty. A full key of the MCP server still reads the console.

### Adopter action

An instance whose public site should keep showing the live console sets `TEHAS_SITE_CONSOLE=on` in `instance.env`, then runs the docs server's `docker compose ... up -d` line again.
