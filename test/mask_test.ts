// Tests the masking regexes used by bin/tehas-console.ts.
//
// tehas-console.ts runs a read-loop at the top level, so it cannot be
// imported from a test. The mask() logic is duplicated here; the two must
// stay in sync (they are the only place in the repo that masks secrets for
// the dashboard console).
//
//   deno test -A --no-config test/mask_test.ts
import { assert, assertEquals } from "jsr:@std/assert@^1.0.19";

function mask(s: string): string {
  return s
    .replace(/\b(gh[pousr]_|github_pat_)[A-Za-z0-9_]{16,}/g, "$1***")
    .replace(/(postgres(?:ql)?:\/\/[^:\s/]+:)[^@\s]+@/g, "$1***@")
    .replace(
      /\b([A-Z0-9_]*(?:TOKEN|SECRET|PASSWORD|API_KEY)[A-Z0-9_]*=)\S+/g,
      "$1***",
    );
}

Deno.test("masks a fine-grained GitHub token", () => {
  const out = mask("token is github_pat_ABC123DEF456GHI789JOK012 and done");
  assertEquals(out, "token is github_pat_*** and done");
  assert(!out.includes("ABC123DEF456"));
});

Deno.test("masks classic GitHub tokens", () => {
  assertEquals(mask("x=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZ"), "x=ghp_***");
  assertEquals(mask("x=gho_ABCDEFGHIJKLMNOPQRSTUVWXYZ"), "x=gho_***");
  assertEquals(mask("x=ghu_ABCDEFGHIJKLMNOPQRSTUVWXYZ"), "x=ghu_***");
  assertEquals(mask("x=ghs_ABCDEFGHIJKLMNOPQRSTUVWXYZ"), "x=ghs_***");
  assertEquals(mask("x=ghr_ABCDEFGHIJKLMNOPQRSTUVWXYZ"), "x=ghr_***");
});

Deno.test("does not mask a short or incomplete GitHub token prefix", () => {
  // Fewer than 16 secret characters: the prefix is not a real token.
  assertEquals(mask("github_pat_short123"), "github_pat_short123");
});

Deno.test("masks the password in a postgres URL", () => {
  assertEquals(
    mask("DB=postgres://tehas:supersecretvalue@db:5432/prod"),
    "DB=postgres://tehas:***@db:5432/prod",
  );
  assertEquals(
    mask("DB=postgresql://user:hunter2@host:5432/db"),
    "DB=postgresql://user:***@host:5432/db",
  );
});

Deno.test("mashes KEY=VALUE secrets for token-like names", () => {
  assertEquals(mask("FOO_TOKEN=abcdef123456"), "FOO_TOKEN=***");
  assertEquals(mask("DATABASE_PASSWORD=hunter2 x"), "DATABASE_PASSWORD=*** x");
  assertEquals(mask("GH_TOKEN=ghp_whatever"), "GH_TOKEN=***");
  assertEquals(mask("CF_API_KEY=s3cr3t"), "CF_API_KEY=***");
});

Deno.test("leaves ordinary text untouched", () => {
  const plain = "no secrets here, just a postgres://user@host:5432/db url";
  assertEquals(mask(plain), plain);
});
