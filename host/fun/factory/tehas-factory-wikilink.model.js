// The dashboard's link to the current wiki build, from live/wiki.json. A build
// is linked only by an address of its own form, /wiki/<date>-<8 hex>/; the
// wiki page holds to the same rule.
export const BUILD = /^\/wiki\/\d{4}-\d{2}-\d{2}-[0-9a-f]{8}\/$/;

export function wikiLink(file) {
  const c = file && file.api === 1 && file.current;
  if (!c || typeof c.url !== "string" || !BUILD.test(c.url)) return null;
  return { href: c.url, title: typeof c.id === "string" ? c.id : "" };
}
