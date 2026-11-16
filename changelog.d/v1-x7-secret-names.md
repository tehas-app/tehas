### Changes

- One rule says which setting names are secret: a name that holds TOKEN, SECRET, PASSWORD, PASSWD, KEY or CREDENTIAL, in any case. The console's mask and `tehas review` use the same rule.
- `tehas review` takes a setting of a documented family, such as `PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST`, when the image lists the family, instead of stopping with exit 78.
- The MCP tool `factory_console` masks the console tail it returns, as the log tools do.
