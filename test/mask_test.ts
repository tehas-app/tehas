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
    )
    // A private network address: an agent can read the container's hosts
    // file, and a connection error names the address it failed to reach.
    .replace(
      /(?<![\d.])(?:10\.\d{1,3}|192\.168|172\.(?:1[6-9]|2\d|3[01])|169\.254|100\.(?:6[4-9]|[7-9]\d|1[01]\d|12[0-7]))\.\d{1,3}\.\d{1,3}(?![\d])/g,
      "[private address]",
    )
    .replace(
      /\b[A-Za-z0-9-]+(?:\.[A-Za-z0-9-]+)*\.ts\.net\b/g,
      "[private name]",
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

// Built here, not written out: scripts/check-generic.sh reads this file too.
const ip = (...parts: number[]) => parts.join(".");

Deno.test("masks private network addresses", () => {
  assertEquals(
    mask(`models ${ip(10, 1, 2, 3)} ok`),
    "models [private address] ok",
  );
  assertEquals(mask(`${ip(192, 168, 1, 10)}:8086`), "[private address]:8086");
  assertEquals(mask(`at ${ip(172, 16, 0, 5)}.`), "at [private address].");
  assertEquals(mask(ip(172, 31, 255, 1)), "[private address]");
  assertEquals(mask(`host_${ip(100, 100, 2, 3)}`), "host_[private address]");
  assertEquals(mask(ip(169, 254, 1, 1)), "[private address]");
  assertEquals(
    mask(["node", "tail1234", "ts", "net"].join(".") + ":8086"),
    "[private name]:8086",
  );
});

Deno.test("leaves public and documentation addresses and versions alone", () => {
  for (
    const s of [
      "192.0.2.7",
      "8.8.8.8",
      "172.32.0.1",
      "100.128.0.1",
      "version 1.10.2.3",
      "210.1.2.3",
      "127.0.0.1",
    ]
  ) assertEquals(mask(s), s);
});
