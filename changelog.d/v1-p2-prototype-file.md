### Changes

- `tehas prototype file` opens a ticket for the Protolab with the ticket text, a `Zip:` line and a `Spec: <link> sha256:<hex>` line, and labels it `tehas:prototype`, all with the caller's own gh login; a gh signed in as the bot, or a login off `TEHAS_ALLOWED_LABELERS`, is refused with exit 3. Still being built: `TEHAS_PROTOTYPE_FILE=on` is refused with exit 78 and `flags` lists `prototype-file` as `false`.
