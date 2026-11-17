### Changes

- With outsource on, the contractors show where a person and a program look: `tehasbot status` writes `live/contractors.json`, `tehas status` prints an `outsource:` line and says of each running cycle whether it is outsourced to a contractor or in-house, `tehas status --json` has an optional `contractors` object (with a new sample, `api/status.outsourced.json`), and the MCP server has `factory_contractors`, served to dashboard keys too. With outsource off nothing changes: no file, no field, no line and no tool. Outsource is still being built, so no factory has it on yet (#2277).
