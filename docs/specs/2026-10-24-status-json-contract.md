# The contract of tehas status --json

## Why

tehas prints `tehas status --json`. tehas-industry reads it to decide which
factory gets model time. Each side had its own tests, and industry's samples
were written by hand. tehas could rename or drop a field and both suites would
stay green. This spec makes a change on one side fail a test.

## The contract

The contract is the object `tehas status --json` prints, as `host/docs/api.md`
describes it. Three files in `api/` pin it down:

- `api/status.schema.json`, a JSON Schema (draft 2020-12). Every object in it
  is closed, so the schema lists every field tehas can print, with its type and
  whether it can be left out or be null.
- `api/status.busy.json`, a sample with every field present.
- `api/status.quiet.json`, a sample of an idle factory: the fields that can be
  left out are left out, and the ones that can be null are null.

The samples are not written by hand. They are what `console status json`
(`console/src/status.rs`) makes of two health files in `test/fixtures/status-json/`, at a
fixed time. The schema is plain JSON Schema and does not depend on jq, so a
port of `status` to Rust can be checked against the same schema and samples.

## What tehas tests

`test/status_contract_test.sh`:

- makes both samples again and fails if either differs from the committed file;
- checks both samples against the schema;
- runs `libexec/summary --json` on the same health files and compares, but for
  the fields that depend on the clock;
- fails if a field of the schema is in neither sample, or has no row in
  `host/docs/api.md`.

The schema check is `test/schema.jq`, about forty lines of jq that know the
keywords the schema uses and refuse any other. The gate gains no dependency.

So a change to the object cannot pass on its own. It must rewrite the samples,
which shows the change in review, and update the schema and the manual.

## What industry tests

Industry copies both samples into `crates/factory-api/tests/fixtures/tehas/`,
with a `SOURCE` file that names the tehas tag and commit they came from. A test
parses each through `Status::parse`, the type the loop uses, and asserts the
fields industry relies on. When industry moves to a newer tehas release, it
copies the samples from that tag. If tehas broke something industry reads, the
copy fails industry's test before the new release is used.

## Breaking changes

The object keeps its `"api": 1` field and the rule `host/docs/api.md` already
states: within version 1 fields are only added, never renamed, removed or given
a new meaning. Industry already refuses any other version. We keep this rather
than invent a second scheme, because it is already published and enforced on
both sides.

- An added field goes into the code, the schema, a sample and the manual in one
  commit. Industry ignores fields it does not know, so nothing breaks.
- Anything else is a breaking change. It sets `"api": 2`, replaces the schema
  and the samples, and the release notes say so. Industry sees a sample it
  refuses and must be changed before it moves to that release.

## Limits

Industry sees a change only when it copies the samples again. Between tehas
releases the two can differ, which is fine while factories run released tags.
The samples show one shape each; they do not prove every value the code can
print. The schema covers that, for tehas only.
