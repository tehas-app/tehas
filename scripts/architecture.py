#!/usr/bin/env python3
# Copyright 2026 Jani Tarvainen
# SPDX-License-Identifier: Apache-2.0
"""The architecture test: the mechanical checks of tehas-standard v2.

    scripts/architecture.py [--config FILE] [--base REF] [--summary FILE] [--root DIR] [--force]

The rules are tehas-standard v2's coding standard, sections 4 and 5
(https://github.com/tehas-app/tehas-standard, `v2/coding.md`). This script
is one file, the same in every repository that follows v2: tehas-standard's
`tools/architecture.py`, copied byte for byte to `scripts/architecture.py`,
its sha256 the one in `tools/architecture.py.sha256`. What it checks is the
repository's own `architecture.toml`.

Each `[[check]]` counts the places a regular expression matches in the
tracked files it names, outside the files where that knowledge is allowed.
The count is a ratchet:

- above its `max`, the check fails: the change spread knowledge or untyped
  data further;
- below its `max`, the check fails too, and says the new number: the pull
  request that lowers a count lowers its `max`, so it cannot rise again;
- a check whose `max` is above 0 must say `why`: the reason the count is
  not 0 yet, and what lowers it;
- with `--base REF` (the pull request's base), a `max` raised over the
  base's must come with a `why` that differs from the base's, and so must
  a check whose scope changed: any key that decides what is counted
  (`pattern`, `standard`, `only`, `never`, `files`, `allowed`, `mode`,
  `start`, `end`, `comment`), whose `why` must also not be empty, even at
  a `max` of 0. A check that is in the base must still be here.

A `[[report]]` counts and lists without failing, for what is judged in
review rather than checked (public items per module, for example). With
`--base`, a report in the base must still be here, and one whose scope
changed (its `top` included) must come with a new, non-empty `why`.

A check (or a report) names its `pattern`, or a `standard`: one of the
standard patterns below, the same in every repository, so the common
checks read their rules alike everywhere. A standard decides what
is counted on its own; a check that names one may not set pattern, mode,
start, end or comment. The Rust standards read the code without its
comments and with string contents blanked.
Three standards are allowlists and take `only` (what may be reached) and
`never` (what may not, though `only` allows it): `rust-inward` (module
paths from the crate root, through `crate::`, `super::`, `self::`, grouped
and multi-line `use`, inline modules and `mod x;`), `js-inward` (the
repository paths of relative imports, as globs, extensionless as written)
and `cargo-deps` (the packages a Cargo.toml depends on, dev-dependencies
left out). The others count fixed shapes: `rust-untyped-signature` (a signature
naming `Value`, across lines), `rust-untyped-field` (`Value` in a pub or
pub(...) field, an enum variant, a tuple struct or newtype, or an alias)
and `rust-public-item` (pub and pub(...) items, `pub use` and exported
macros included).

Apart from the standards, nothing here knows a language, and nothing knows
a repository. Modes: `lines` (default)
counts each line that matches. `spans` counts each span of lines that
opens on a line matching `start` and closes on the first line matching
`end` (a function's signature, say: from `fn` to `{` in Rust, from
`function` to `{` in JavaScript), once, if it matches anywhere in it; the
text from the `end` match on is left out. Lines whose first characters are
the check's `comment` prefix are skipped.

The repository says which standard it follows in `tehas.toml` at `--root`,
table `[standard]`: `url`, tehas-standard's, and `version`. Before v2 the
pointer was the one line of `.tehas/coding-standard`; it is read only for
`v1`, and the two files are never kept at once. `v1`, or no file, is the
code as it was before its first refactoring runs: the test says it is
skipped and passes; `--force` runs it anyway, to see the counts. `v2` is
enforced. A later standard (`v2.1`, `v3`) comes with the version of this
script that enforces it, and this one refuses it by name. Any other value,
a file with no `[standard]` table, an unknown key in it, and a symlink are
refused. `[platform]` names the toolchains, each at a major v2 allows; the
other tables of `tehas.toml` have other owners. Both files are read once.

With `--base`, the base must be a commit here, and its files are read by
their git mode. An adopted standard only goes up: the head may not name a
lower standard than the base (by the higher of the base's two files, where
it has both), or none where the base names one, and a `[platform]` the base
names may not lose a toolchain. A base whose pointer is there but cannot be
read (not TOML, not UTF-8, a value that is no standard) is not a reason to
block its repair: the head must then name v2, the highest standard this
script knows, so nothing can go down, and the summary says the base was
not compared. A run without `--base` (a push) compares with nothing: the
pull request's run guards the step, and a workflow that wants the guard on
a push passes the push's previous commit as `--base`.

Exit codes: 0 every check holds or the test is skipped, 1 a check fails, 2 the configuration or
the repository cannot be read. Needs python3 (3.11 or later) and git. No
network.
"""
# tehas-standard v2

import argparse
import os
import posixpath
import re
import subprocess
import sys
import tomllib

CHECK_KEYS = {"id", "rule", "what", "pattern", "standard", "only", "never", "files", "allowed", "max", "why", "mode", "comment", "start", "end"}
CHECK_REQUIRED = {"id", "rule", "what", "files", "max"}
# Every key but these decides what a check counts, so changing it moves
# the count as surely as raising max does.
NOT_SCOPE = {"id", "rule", "what", "max", "why"}
SCOPE_DEFAULTS = {"allowed": [], "mode": "lines", "top": 10, "only": [], "never": []}
REPORT_KEYS = {"id", "rule", "what", "pattern", "standard", "only", "never", "files", "allowed", "top", "mode", "comment", "start", "end", "why"}
REPORT_REQUIRED = {"id", "rule", "what", "files"}
# What a standard fixes, so a check that names one cannot narrow it.
FIXED_BY_STANDARD = {"pattern", "mode", "comment", "start", "end"}
TOP_KEYS = {"version", "method", "review", "check", "report"}
MODES = {"lines", "spans"}
RULE = re.compile(r"R([1-9]|10)")
SHOWN = 20

# The pointer, tehas.toml: this script owns [standard] and, for now,
# [platform]; other tables have other owners.
STANDARD_FILE = "tehas.toml"
OLD_STANDARD_FILE = ".tehas/coding-standard"  # the pointer's name before v2; read only for v1
STANDARD_KEYS = {"url", "version"}
STANDARD_URL = "https://github.com/tehas-app/tehas-standard"  # the standard this script checks
PLATFORM_ALLOWED = {"rust": {"1.x"}, "typescript": {"7.x"}}  # each toolchain's majors at v2
STANDARDS = {"v1", "v2"}
ENFORCED = "v2"  # also the highest STANDARDS holds

class ConfigError(Exception):
    pass


class Unreadable(ConfigError):
    """A pointer file that is there but says nothing this script can read:
    not UTF-8, not TOML, or a value that is no standard. At the head it is
    refused like any ConfigError; at the base it leads to the repair path."""


def git_bytes(*args, cwd="."):
    r = subprocess.run(["git", *args], cwd=cwd, capture_output=True)
    if r.returncode != 0:
        raise ConfigError(f"git {' '.join(args)}: {r.stderr.decode('utf-8', 'replace').strip() or 'failed'}")
    return r.stdout


def git(*args, cwd="."):
    # surrogateescape: a path that is not UTF-8 still names its file
    return git_bytes(*args, cwd=cwd).decode("utf-8", "surrogateescape")


def decoded(data, name):
    try:
        return data.decode("utf-8")
    except UnicodeDecodeError:
        raise Unreadable(f"{name}: not UTF-8")


def glob_re(pattern):
    """A path glob as a regex: `**` crosses directories, `*` does not."""
    out, i = "", 0
    while i < len(pattern):
        if pattern.startswith("**/", i):
            out += "(?:.*/)?"
            i += 3
        elif pattern.startswith("**", i):
            out += ".*"
            i += 2
        elif pattern[i] == "*":
            out += "[^/]*"
            i += 1
        elif pattern[i] == "?":
            out += "[^/]"
            i += 1
        else:
            out += re.escape(pattern[i])
            i += 1
    return re.compile(out + r"\Z")


def matches_any(path, globs):
    return any(g.match(path) for g in globs)


def parse(text, where):
    """The configuration, every problem at once."""
    try:
        doc = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise ConfigError(f"{where}: {e}")
    problems = []
    for k in doc:
        if k not in TOP_KEYS:
            problems.append(f"{where}: unknown key {k!r}")
    if doc.get("version") != 1:
        problems.append(f"{where}: version must be 1")
    seen = set()
    for kind, keys, required in (("check", CHECK_KEYS, CHECK_REQUIRED), ("report", REPORT_KEYS, REPORT_REQUIRED)):
        entries = doc.get(kind, [])
        if not isinstance(entries, list):
            problems.append(f"{where}: {kind} must be an array of tables ([[{kind}]])")
            continue
        for n, c in enumerate(entries, 1):
            name = f"{where}: {kind} {c.get('id', n)!s}"
            for k in c:
                if k not in keys:
                    problems.append(f"{name}: unknown key {k!r}")
            for k in sorted(required - c.keys()):
                problems.append(f"{name}: {k} is missing")
            cid = c.get("id")
            if isinstance(cid, str):
                if cid in seen:
                    problems.append(f"{name}: the id is used twice")
                seen.add(cid)
            if "rule" in c and not (isinstance(c["rule"], str) and RULE.fullmatch(c["rule"])):
                problems.append(f"{name}: rule must be one of R1 to R10")
            std = c.get("standard")
            if ("pattern" in c) == ("standard" in c):
                problems.append(f"{name}: name exactly one of pattern and standard")
            if std is not None:
                if not isinstance(std, str) or std not in STANDARD_PATTERNS:
                    problems.append(f"{name}: standard must be one of {', '.join(sorted(STANDARD_PATTERNS))}")
                else:
                    for k in sorted(FIXED_BY_STANDARD & c.keys()):
                        problems.append(f"{name}: {k} is fixed by the standard {std}; leave it out")
                    takes_only = STANDARD_PATTERNS[std].get("only", False)
                    if takes_only and not c.get("only"):
                        problems.append(f"{name}: the standard {std} needs only, the list of what may be used")
                    if not takes_only:
                        for k in ("only", "never"):
                            if k in c:
                                problems.append(f"{name}: {k} is for the standards that take a list ({', '.join(sorted(s for s, d in STANDARD_PATTERNS.items() if d.get('only')))})")
            elif "only" in c or "never" in c:
                problems.append(f"{name}: only and never need a standard")
            for k in ("files", "allowed", "only", "never"):
                v = c.get(k, [])
                if not (isinstance(v, list) and all(isinstance(x, str) and x for x in v)):
                    problems.append(f"{name}: {k} must be a list of non-empty strings")
            if kind == "check" and "files" in c and not c["files"]:
                problems.append(f"{name}: files must name at least one glob")
            if "max" in c and not (isinstance(c["max"], int) and not isinstance(c["max"], bool) and c["max"] >= 0):
                problems.append(f"{name}: max must be a whole number, 0 or more")
            if "top" in c and not (isinstance(c["top"], int) and c["top"] > 0):
                problems.append(f"{name}: top must be a whole number above 0")
            if c.get("mode", "lines") not in MODES:
                problems.append(f"{name}: mode must be one of {', '.join(sorted(MODES))}")
            for k in ("pattern", "start", "end"):
                if k in c:
                    try:
                        re.compile(c[k])
                    except (re.error, TypeError) as e:
                        problems.append(f"{name}: {k}: {e}")
            if c.get("mode") == "spans":
                for k in ("start", "end"):
                    if k not in c:
                        problems.append(f"{name}: mode spans needs {k}")
            elif "start" in c or "end" in c:
                problems.append(f"{name}: start and end are for mode spans")
            for k in ("what", "why", "comment"):
                if k in c and not isinstance(c[k], str):
                    problems.append(f"{name}: {k} must be text")
    if problems:
        raise ConfigError("\n".join(problems))
    return doc


def scope(entry, known=CHECK_KEYS):
    """What decides a check's count (or a report's list), with the defaults
    written out, so a key left out and the same key at its default compare
    equal."""
    keys = (known - NOT_SCOPE) | (entry.keys() - NOT_SCOPE)
    return {k: entry.get(k, SCOPE_DEFAULTS.get(k)) for k in sorted(keys)}


def rescoped(entry, base, known=CHECK_KEYS):
    """The scope keys that differ between a check (or a report) and its base."""
    a, b = scope(entry, known), scope(base, known)
    return [k for k in sorted(a.keys() | b.keys()) if a.get(k) != b.get(k)]


def units(text, entry):
    """(line number, text) for each line, or for each span."""
    comment = entry.get("comment")
    lines = text.split("\n")
    keep = [(n, l) for n, l in enumerate(lines, 1) if not (comment and l.lstrip().startswith(comment))]
    if entry.get("mode", "lines") == "lines":
        yield from keep
        return
    opens, closes = re.compile(entry["start"]), re.compile(entry["end"])
    first, acc = None, []
    for n, l in keep:
        if first is None:
            m = opens.search(l)
            if not m:
                continue
            first, acc, l = n, [], l[m.start():]
        e = closes.search(l)
        acc.append(l[: e.start()] if e else l)
        if e:
            yield first, " ".join(acc)
            first = None


# The standard patterns (see the module's docstring). This is the one
# place the script knows a language. Each is a function of a file's path,
# its text and the check, giving (line number, text) for each place counted.
#
# `Value` is serde_json's untyped document (`serde_json::Value` included).
# A path into it (`Value::Null`) is a use, not a type, so it is not counted.
VALUE = re.compile(r"\bValue\b(?!\s*::)")
PUB = r"pub(\s*\([^)]*\))?\s+"
ATTRS = r"(#\[[^\]]*\]\s*)*"


# Matched in place (`.match(text, i)`), never on a slice, so a long file
# is read once.
RAW_STRING = re.compile(r'b?r(#*)"')
CHAR_LITERAL = re.compile(r"'(\\.[^']*|[^\\'])'")

def rust_code(text):
    """The text with comments removed and the insides of string and character
    literals blanked, line breaks kept: what the Rust standards read, so a
    brace, a path or a `Value` in a comment or a string counts for nothing."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif text.startswith("/*", i):
            depth, i = 1, i + 2
            while i < n and depth:
                if text.startswith("/*", i):
                    depth, i = depth + 1, i + 2
                elif text.startswith("*/", i):
                    depth, i = depth - 1, i + 2
                else:
                    if text[i] == "\n":
                        out.append("\n")
                    i += 1
        elif (m := RAW_STRING.match(text, i)) and (i == 0 or not (text[i - 1].isalnum() or text[i - 1] == "_")):
            close = '"' + m.group(1)
            j = text.find(close, m.end())
            j = n if j < 0 else j
            out.append('""' + "\n" * text.count("\n", i, j))
            i = j + len(close)
        elif c == '"':
            j = i + 1
            while j < n and text[j] != '"':
                j += 2 if text[j] == "\\" else 1
            out.append('""' + "\n" * text.count("\n", i, j))
            i = j + 1
        elif c == "'" and (m := CHAR_LITERAL.match(text, i)):
            out.append("''")
            i = m.end()
        else:
            out.append(c)
            i += 1
    return "".join(out).split("\n")


def closing(lines, n, opener):
    """From line index n, the text up to and including the first `;` or
    `{`/`}`-balanced end at bracket depth 0, or the matching close of
    `opener`: (last line index, joined text)."""
    depth, acc = 0, []
    for k in range(n, len(lines)):
        acc.append(lines[k])
        for ch in lines[k]:
            if ch in "([{":
                depth += 1
            elif ch in ")]}":
                depth -= 1
                if opener and depth == 0:
                    return k, " ".join(acc)
            elif ch == ";" and depth == 0 and not opener:
                return k, " ".join(acc)
    return len(lines) - 1, " ".join(acc)


SIG_START = re.compile(rf"(?:^\s*|[{{;}}]\s*){ATTRS}({PUB})?((default|const|async|unsafe|extern(\s+\"[^\"]*\")?)\s+)*fn\s+\w+")


def rust_untyped_signature(path, text, entry, read=None):
    """A function signature naming Value: from `fn` to its body's `{` or its
    `;`, whichever comes first outside brackets and generics, across lines;
    every function on a line."""
    lines, hits = rust_code(text), []
    k, pos = 0, 0
    while k < len(lines):
        m = SIG_START.search(lines[k], pos)
        if not m:
            k, pos = k + 1, 0
            continue
        first, sig, depth, angle, done = k, [], 0, 0, False
        col = m.start() + len(lines[k][m.start():]) - len(lines[k][m.start():].lstrip("{;} \t"))
        rest, at = lines[k][col:], col
        while not done:
            prev = ""
            for j, ch in enumerate(rest):
                if ch in "([":
                    depth += 1
                elif ch in ")]":
                    depth -= 1
                elif ch == "<":
                    angle += 1
                elif ch == ">" and prev not in "-=" and angle:
                    angle -= 1
                elif ch in "{;" and depth == 0 and angle == 0:
                    done, pos = True, at + j     # the next fn may follow it
                    break
                sig.append(ch)
                prev = ch
            sig.append(" ")
            if not done:
                k, at = k + 1, 0
                if k >= len(lines):
                    break
                rest = lines[k]
        if VALUE.search("".join(sig)) and (not hits or hits[-1][0] != first + 1):
            hits.append((first + 1, lines[first].strip()))
        if not done:
            break
    return hits


def statement(lines, k, col=0, stops=";{", generics=False):
    """From line k, column col: the text to the first character of `stops`
    outside brackets (and, for a type, outside `<...>`), across lines:
    (that character or "", the text)."""
    depth, angle, acc, prev = 0, 0, [], ""
    for i in range(k, len(lines)):
        line = lines[i][col:] if i == k else lines[i]
        for ch in line:
            if ch in "([" or (ch == "{" and "{" not in stops):
                depth += 1
            elif ch in ")]" or (ch == "}" and "}" not in stops):
                if depth == 0 and angle == 0 and ch in stops:
                    return ch, "".join(acc)
                depth -= 1
            elif generics and ch == "<":
                angle += 1
            elif generics and ch == ">" and prev not in "-=" and angle:
                angle -= 1
            elif ch in stops and depth == 0 and angle == 0:
                return ch, "".join(acc)
            acc.append(ch)
            prev = ch
        acc.append(" ")
    return "", "".join(acc)


PUB_FIELD = re.compile(rf"(?:^\s*|[{{,(]\s*){ATTRS}{PUB}(r#)?[a-z_]\w*\s*:")
ENUM_OPEN = re.compile(rf"^\s*{ATTRS}({PUB})?enum\s+\w+")
STRUCT_OPEN = re.compile(rf"^\s*{ATTRS}({PUB})?struct\s+\w+")
TYPE_ALIAS = re.compile(rf"^\s*{ATTRS}({PUB})?type\s+\w+")
VARIANT = re.compile(rf"^\s*{ATTRS}[A-Z]\w*\s*(?P<open>[({{])")


def rust_untyped_field(path, text, entry, read=None):
    """Value held where it can cross a seam: a pub or pub(...) field, an
    enum's tuple or struct variant (always public), a tuple struct or newtype
    (its field private or not), a type alias; each across lines."""
    lines, hits = rust_code(text), set()
    depth, enums, pending = 0, [], False   # enums: the depth of each enum body
    for k, line in enumerate(lines):
        for m in PUB_FIELD.finditer(line):
            # the field's own type: to the `,`, `}` or `)` that ends it
            _, ty = statement(lines, k, m.end(), ",})", generics=True)
            if VALUE.search(ty):
                hits.add(k + 1)
        if STRUCT_OPEN.match(line):
            end, whole = statement(lines, k)
            if end == ";" and VALUE.search(whole):    # a tuple struct
                hits.add(k + 1)
        if TYPE_ALIAS.match(line):
            _, whole = statement(lines, k, 0, ";")
            if "=" in whole and VALUE.search(whole.split("=", 1)[1]):
                hits.add(k + 1)
        if enums and depth == enums[-1] and (m := VARIANT.match(line)):
            _, whole = closing(lines, k, m.group("open"))
            if VALUE.search(whole):
                hits.add(k + 1)
        if ENUM_OPEN.match(line):
            pending = True
        for ch in line:
            if ch == "{":
                depth += 1
                if pending:
                    enums.append(depth)
                    pending = False
            elif ch == "}":
                if enums and depth == enums[-1]:
                    enums.pop()
                depth -= 1
            elif ch == ";":
                pending = False
    return [(n, lines[n - 1].strip()) for n in sorted(hits)]


PUBLIC_ITEM = re.compile(
    rf"^\s*{ATTRS}{PUB}((default|async|const|unsafe|extern(\s+\"[^\"]*\")?)\s+)*"
    r"(fn|struct|enum|union|trait|type|const|static|mod|use|macro|extern\s+crate)\b"
    r"|^\s*#\[macro_export\b")


def rust_public_item(path, text, entry, read=None):
    """A public item: `pub` and `pub(...)` (crate-visible counts, as it is
    reachable from every module), re-exports and exported macros included."""
    return [(n, l.strip()) for n, l in enumerate(rust_code(text), 1) if PUBLIC_ITEM.search(l)]


def under(path, prefixes):
    return any(path == p or path.startswith(p + "::") for p in prefixes)


def rust_crate(path, read=None):
    """(the crate's own name as code writes it, the file's module path from
    its crate root). The root is the last `src/`; else the last `tests/`,
    `examples/` or `benches/`, where each file, or each directory's main.rs,
    is a crate of its own. `lib.rs`, `main.rs` and `mod.rs` name their
    directory. The name is the package's, from the crate's Cargo.toml when
    it can be read, else its directory's."""
    parts_all = path.split("/")
    roots = [i for i, x in enumerate(parts_all[:-1]) if x == "src"]
    if not roots:
        roots = [i for i, x in enumerate(parts_all[:-1]) if x in ("tests", "examples", "benches")]
    if not roots:
        return "", [x for x in parts_all[1:]][:-1]
    i = roots[-1]
    crate_dir, rel = parts_all[:i], parts_all[i + 1:]
    if parts_all[i] != "src":
        rel = [] if len(rel) == 1 else rel[1:]
    else:
        rel = rel
    if rel and rel[-1].endswith(".rs"):
        rel = rel[:-1] + [rel[-1][:-3]]
    if rel and rel[-1] in ("lib", "main", "mod"):
        rel = rel[:-1]
    name = crate_dir[-1] if crate_dir else ""
    if read is not None:
        toml_text = read("/".join(crate_dir + ["Cargo.toml"]))
        if toml_text:
            try:
                pkg = tomllib.loads(toml_text).get("package", {})
                if isinstance(pkg, dict) and isinstance(pkg.get("name"), str):
                    name = pkg["name"]
            except tomllib.TOMLDecodeError:
                pass
    return name.replace("-", "_"), rel


def use_paths(tree):
    """The full paths a `use` tree names: `a::{b, c::{d, e as f}, self}` is
    a::b, a::c::d, a::c::e and a. A glob names the module it is in."""
    tree = re.sub(r"\s+", "", re.sub(r"\s+as\s+[A-Za-z_]\w*", "", tree.replace("r#", "")))
    out = []

    def walk(prefix, s, i):
        while i < len(s):
            m = re.match(r"(::)?((?:\$?[A-Za-z_]\w*|\*)(?:::(?:[A-Za-z_]\w*|\*))*)?(::)?", s[i:])
            segs = [x for x in (m.group(2) or "").split("::") if x]
            i += m.end()
            if i < len(s) and s[i] == "{":
                i = walk(prefix + segs, s, i + 1)
            elif segs:
                path = prefix + segs
                if path[-1] in ("self", "*"):
                    path = path[:-1]
                out.append(path)
            if i < len(s) and s[i] == ",":
                i += 1
            elif i < len(s) and s[i] == "}":
                return i + 1
            elif i < len(s) and not m.end():
                i += 1            # a character no tree has: skip it
        return i

    walk([], tree, 0)
    return out


RUST_PATH = re.compile(r"(?<![\w$])(\$crate|crate|self|super)((?:\s*::\s*super)*)\s*::\s*(\*|\{|[A-Za-z_]\w*(?:\s*::\s*[A-Za-z_]\w*)*)")
RUST_MOD = re.compile(rf"^\s*{ATTRS}({PUB})?mod\s+(r#)?(?P<name>[a-z_]\w*)\s*;")
RUST_INLINE_MOD = re.compile(rf"^\s*{ATTRS}({PUB})?mod\s+(r#)?(?P<name>[a-z_]\w*)\s*\{{")
RUST_USE = re.compile(rf"(?:^\s*|[{{;}}]\s*){ATTRS}({PUB})?use\s+")
ROOT = "crate"


def resolve(path, here, children, own):
    """A path as a module path from the crate root (`[]` for the root
    itself), or None for another crate."""
    if not path:
        return None
    head, rest = path[0], path[1:]
    if head in ("crate", "$crate") or (own and head == own):
        return rest
    if head in ("self", "super"):
        base = list(here)
        while path and path[0] in ("self", "super"):
            if path[0] == "super":
                base = base[:-1]
            path = path[1:]
        return base + path
    if head in children:
        return list(here) + path
    return None


def rust_inward(path, text, entry, read=None):
    """The modules a file reaches, as an allowlist. The crate root itself
    (`use crate::*`, `use super::*` from a top module) is `crate` in `only`."""
    own, top = rust_crate(path, read)
    keep = list(enumerate(rust_code(text), 1))
    children = {m.group("name") for _, l in keep if (m := RUST_MOD.match(l))}
    only, never = entry["only"], entry.get("never", [])
    inside, stack, depth = {}, [], 0
    for n, l in keep:
        inside[n] = [name for name, _ in stack]
        m = RUST_INLINE_MOD.match(l)
        before = depth
        for ch in l:
            if ch == "{":
                depth += 1
            elif ch == "}":
                depth -= 1
                while stack and depth < stack[-1][1]:
                    stack.pop()
        if m and depth > before:
            stack.append((m.group("name"), before + 1))
    hits, i = [], 0
    while i < len(keep):
        n, l = keep[i]
        here = top + inside.get(n, [])
        reached, rest = [], l
        u = RUST_USE.search(l)
        if u:
            acc, rest = [l[u.end():]], l[:u.start()]
            while ";" not in acc[-1] and i + 1 < len(keep):
                i += 1
                acc.append(keep[i][1])
            tree, _, after = " ".join(x.strip() for x in acc).partition(";")
            rest += " " + after
            for p in use_paths(tree):
                r = resolve(p, here, children, own)
                if r is not None:
                    reached.append(r)
        m = RUST_MOD.match(l)
        if m:
            reached.append(here + [m.group("name")])
        for m in RUST_PATH.finditer(rest):
            head = [m.group(1)] + re.findall(r"super", m.group(2))
            if m.group(3) == "{":
                _, tree = statement([rest], 0, m.end(), "}")
                paths = [head + p for p in use_paths(tree)] or [head]
            else:
                paths = [head + [x for x in re.split(r"\s*::\s*", m.group(3)) if x != "*"]]
            for p in paths:
                r = resolve(p, here, children, own)
                if r is not None:
                    reached.append(r)
        names = ["::".join(x) or ROOT for x in reached]
        if any(not under(r, only) or under(r, never) for r in names):
            hits.append((n, l.strip()))
        i += 1
    return hits


def js_code(text):
    """JavaScript with `//` comments and non-JSDoc `/* */` comments removed,
    strings and template literals kept as written, line breaks kept. A JSDoc
    comment stays: its type imports are dependencies too."""
    out, i, n = [], 0, len(text)
    while i < n:
        c = text[i]
        if c in "\"'`":
            j = i + 1
            while j < n and text[j] != c and not (c != "`" and text[j] == "\n"):
                j += 2 if text[j] == "\\" else 1
            out.append(text[i:j + 1])
            i = j + 1
        elif text.startswith("//", i):
            j = text.find("\n", i)
            i = n if j < 0 else j
        elif text.startswith("/*", i) and not text.startswith("/**", i):
            j = text.find("*/", i + 2)
            j = n if j < 0 else j + 2
            out.append("\n" * text.count("\n", i, j))
            i = j
        else:
            out.append(c)
            i += 1
    return "".join(out)


JS_SPEC = r"""(?:"(\.{1,2}(?:/[^"]*)?)"|'(\.{1,2}(?:/[^']*)?)'|`(\.{1,2}(?:/[^`]*)?)`)"""
JS_IMPORT = re.compile(rf"\bfrom\s*{JS_SPEC}|\bimport\s*\(?\s*{JS_SPEC}|\brequire\s*\(\s*{JS_SPEC}")


def js_inward(path, text, entry, read=None):
    """The files a JavaScript module imports, as an allowlist of repository
    globs: `from`, `import`, `import()`, `require()` and `export ... from`
    with a relative specifier, a JSDoc type import included. A computed
    specifier (`${...}`) is judged by the directory before it. An absolute
    specifier names a served path, not a file, and is not resolved."""
    lines = js_code(text).split("\n")
    shown = text.split("\n")
    only = [glob_re(g) for g in entry["only"]]
    never = [glob_re(g) for g in entry.get("never", [])]
    here = posixpath.dirname(path)
    hits = []
    for n, l in enumerate(lines, 1):
        seen = l + (" " + lines[n] if re.search(r"\b(from|import|require\s*\()\s*$", l) and n < len(lines) else "")
        for m in JS_IMPORT.finditer(seen):
            spec = next(g for g in m.groups() if g is not None)
            if "${" in spec:
                spec = spec.split("${", 1)[0].rsplit("/", 1)[0] + "/*"
            target = posixpath.normpath(posixpath.join(here, spec))
            ok = matches_any(target, only) or matches_any(target + "/", only)
            if not ok or matches_any(target, never):
                hits.append((n, shown[n - 1] if n <= len(shown) else l))
                break
    return hits


def cargo_deps(path, text, entry, read=None):
    """A Cargo.toml's dependencies, as an allowlist of package names. A
    workspace-inherited dependency is judged by its key."""
    try:
        doc = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise ConfigError(f"{path}: {e}")
    table = lambda d, k: d.get(k) if isinstance(d.get(k), dict) else {}
    tables = [table(doc, "dependencies"), table(doc, "build-dependencies")]
    for t in table(doc, "target").values():
        if isinstance(t, dict):
            tables += [table(t, "dependencies"), table(t, "build-dependencies")]
    only = [glob_re(g) for g in entry["only"]]
    never = [glob_re(g) for g in entry.get("never", [])]
    lines = text.split("\n")
    hits = []
    for t in tables:
        for key, spec in t.items():
            name = spec.get("package", key) if isinstance(spec, dict) else key
            if not isinstance(name, str) or (matches_any(name, only) and not matches_any(name, never)):
                continue
            k = re.escape(key)
            n = next((i for i, l in enumerate(lines, 1)
                      if re.match(rf"""^\s*["']?{k}["']?\s*(\.|=)""", l) or re.match(rf"""^\s*\[.*dependencies\.["']?{k}["']?\]""", l)), 0)
            hits.append((n, lines[n - 1] if n else key))
    return hits


STANDARD_PATTERNS = {
    "rust-untyped-signature": {"count": rust_untyped_signature},
    "rust-untyped-field": {"count": rust_untyped_field},
    "rust-public-item": {"count": rust_public_item},
    "rust-inward": {"count": rust_inward, "only": True},
    "js-inward": {"count": js_inward, "only": True},
    "cargo-deps": {"count": cargo_deps, "only": True},
}


def count(entry, files, read):
    """The places `entry` matches outside its allowed files."""
    std = STANDARD_PATTERNS.get(entry.get("standard"))
    by_list = std["count"] if std else None
    pattern = None if by_list else re.compile(entry["pattern"])
    include = [glob_re(g) for g in entry["files"]]
    allowed = [glob_re(g) for g in entry.get("allowed", [])]
    hits = []
    for path in files:
        if not matches_any(path, include) or matches_any(path, allowed):
            continue
        text = read(path)
        if text is None:
            continue
        if by_list:
            # shown as written, not as the standard read it
            lines = text.split("\n")
            hits += [(path, n, shown((lines[n - 1] if 0 < n <= len(lines) else line).strip()))
                     for n, line in by_list(path, text, entry, read)]
            continue
        for n, line in units(text, entry):
            if pattern.search(line):
                hits.append((path, n, shown(line.strip())))
    return hits




def tracked(root):
    return [p for p in git("ls-files", "-z", cwd=root).split("\0") if p]


def text_of(path, name):
    """The UTF-8 text of the regular file at `path` (through a symlink), or
    ConfigError naming `name`: gone, a directory, another kind of file, or
    not UTF-8."""
    if os.path.isdir(path):
        raise ConfigError(f"{name}: a directory, not a file")
    if not os.path.isfile(path):
        raise ConfigError(f"{name}: not a regular file")
    with open(path, "rb") as f:
        return decoded(f.read(), name)


def reader(root):
    """What the checks count in: a tracked file's text, or None when there is
    no file to read (gone, a directory). A byte that is not UTF-8 is kept as
    a surrogate (surrogateescape), so a file is always counted: one bad byte
    must not hide the matches around it. A file that is there but cannot be
    read (its permissions, say) is a ConfigError, never "no text"."""
    def read(path):
        full = os.path.join(root, path)
        if os.path.isdir(full) or not os.path.isfile(full):
            return None
        try:
            with open(full, "rb") as f:
                return f.read().decode("utf-8", "surrogateescape")
        except OSError as e:
            raise ConfigError(f"{path}: tracked, but cannot be read ({e.strerror or e}); "
                              f"a file the checks cannot read is never counted as empty")
    return read


def shown(line):
    """A counted line as it is printed: a byte that was not UTF-8 shows as
    U+FFFD, so printing never fails."""
    return line.encode("utf-8", "surrogateescape").decode("utf-8", "replace")


class WorkTree:
    """The head: the files under --root as they are now."""

    def __init__(self, root):
        self.root = root

    def where(self, name):
        return name

    def read(self, name):
        """The text of `name`, or None when there is no such path. A symlink,
        at the path or on the way to it, a directory and text that is not
        UTF-8 are refused, so none of them can hide the standard."""
        parts = name.split("/")
        for i in range(1, len(parts)):
            if os.path.islink(os.path.join(self.root, *parts[:i])):
                raise ConfigError(f"{name}: a symlink, or under one ({'/'.join(parts[:i])}); write the file itself")
        path = os.path.join(self.root, name)
        if os.path.islink(path):
            raise ConfigError(f"{name}: a symlink; write the file itself")
        if not os.path.lexists(path):
            return None
        return text_of(path, name)


class Commit:
    """The base: the files under --root at a commit, read by their git mode.
    Every path is relative to --root, for `ls-tree` and `show` alike."""

    def __init__(self, root, ref):
        self.root, self.ref = root, ref

    def where(self, name):
        return f"{self.ref}:{name}"

    def read(self, name):
        """The text of `name` at the commit, or None when it has no such
        path. Anything but a regular file there, or a symlinked directory on
        the way, is refused, so it can't hide the base's standard."""
        parts = name.split("/")
        for i in range(1, len(parts) + 1):
            sub = "/".join(parts[:i])
            entry = git("ls-tree", self.ref, "--", sub, cwd=self.root).strip()
            if not entry:
                return None
            mode = entry.split()[0]
            if i < len(parts) and mode == "120000":
                raise ConfigError(f"{self.where(sub)}: a symlink")
            if i < len(parts) and mode != "040000":
                return None
        if mode not in ("100644", "100755"):
            raise ConfigError(f"{self.where(name)}: not a regular file (mode {mode})")
        return decoded(git_bytes("show", f"{self.ref}:./{name}", cwd=self.root), self.where(name))


def run(root, config_path, base, summary, notes=()):
    with open(os.path.join(root, config_path), encoding="utf-8") as f:
        text = f.read()
    doc = parse(text, config_path)
    files = tracked(root)
    read = reader(root)
    failures, rows, notes = [], [], list(notes)

    base_checks = None
    base_reports = None
    if base:
        base_text = Commit(root, base).read(config_path)
        if base_text is None:
            notes.append(f"The base {base} has no {config_path}; nothing to compare with.")
        else:
            base_doc = parse(base_text, f"{base}:{config_path}")
            base_checks = {c["id"]: c for c in base_doc.get("check", [])}
            base_reports = {r["id"]: r for r in base_doc.get("report", [])}

    for c in doc.get("check", []):
        hits = count(c, files, read)
        now, limit = len(hits), c["max"]
        state = "holds"
        if now > limit:
            state = "rose"
            failures.append(f"{c['id']} ({c['rule']}): {now} places, {limit} allowed. {c['what']}.")
            for path, n, line in hits[:SHOWN]:
                failures.append(f"    {path}:{n}: {line[:160]}")
            if len(hits) > SHOWN:
                failures.append(f"    and {len(hits) - SHOWN} more")
        elif now < limit:
            state = "fell"
            failures.append(f"{c['id']} ({c['rule']}): {now} places, fewer than {limit}. Set max = {now} in {config_path}, so it cannot rise again.")
        if limit > 0 and not c.get("why", "").strip():
            state = "no why"
            failures.append(f"{c['id']} ({c['rule']}): max is {limit} with no why. Write in its why why the count is not 0 yet, and the step that lowers it.")
        if base_checks is not None and c["id"] in base_checks:
            b = base_checks[c["id"]]
            same_why = c.get("why", "").strip() == b.get("why", "").strip()
            if limit > b["max"] and same_why:
                state = "raised"
                failures.append(f"{c['id']} ({c['rule']}): max raised from {b['max']} to {limit} without a new why. Write the reason in its why, or keep the count down.")
            changed = rescoped(c, b)
            # A rescope explains itself even at max 0, where "no why" does not apply.
            if changed and (same_why or not c.get("why", "").strip()):
                state = "rescoped"
                failures.append(f"{c['id']} ({c['rule']}): {', '.join(changed)} changed from the base without a new why. Write in its why what the new scope counts and why, or keep the base's.")
        rows.append((c["id"], c["rule"], str(base_checks[c["id"]]["max"]) if base_checks and c["id"] in base_checks else "", str(limit), str(now), state))

    if base_checks is not None:
        here = {c["id"] for c in doc.get("check", [])}
        for cid, b in base_checks.items():
            if cid not in here:
                failures.append(f"{cid} ({b['rule']}): the check is in the base and gone here. Keep it; a departure is a raised max with its why.")

    if base_reports is not None:
        # A report is judged in review, not counted against a max, but what
        # it lists is still a measure: dropping or narrowing it is written.
        here = {r["id"]: r for r in doc.get("report", [])}
        for rid, b in base_reports.items():
            if rid not in here:
                failures.append(f"{rid} ({b['rule']}): the report is in the base and gone here. Keep it, or write why in a new check or report.")
                continue
            r = here[rid]
            changed = rescoped(r, b, REPORT_KEYS)
            why = r.get("why", "").strip()
            if changed and (not why or why == b.get("why", "").strip()):
                failures.append(f"{rid} ({r['rule']}): {', '.join(changed)} changed from the base without a new why. Write in its why what the report now lists and why, or keep the base's.")

    reports = []
    for r in doc.get("report", []):
        per = {}
        for path, _, _ in count(r, files, read):
            per[path] = per.get(path, 0) + 1
        top = sorted(per.items(), key=lambda kv: (-kv[1], kv[0]))[: r.get("top", 10)]
        reports.append((r, sum(per.values()), top))

    out = ["## Architecture test", ""]
    out += [n + "\n" for n in notes]
    out += ["| Check | Rule | Base max | Max | Now | |", "| --- | --- | --- | --- | --- | --- |"]
    out += [f"| `{a}` | {b} | {c} | {d} | {e} | {f} |" for a, b, c, d, e, f in rows]
    for r, total, top in reports:
        out += ["", f"**{r['id']}** ({r['rule']}, reported, not checked): {r['what']}. {total} in all."]
        out += [f"- `{p}`: {n}" for p, n in top]
    if doc.get("review"):
        out += ["", f"Review the pull request against the questions in `{doc['review']}`."]
    if failures:
        out += ["", "### What fails", "", "```"] + failures + ["```"]
    return write(out, summary, 1 if failures else 0)


def write(out, summary, code):
    text = "\n".join(out) + "\n"
    print(text)
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(text)
    return code


def pointer(tree, history=False):
    """(the standard a tree names, its `[platform]` as written), from
    `tehas.toml` (table `[standard]`: `url`, `version`) and the old one-line
    `.tehas/coding-standard`, each read once. The standard is None when the
    tree has neither file.

    At the head (`history` false) the old file is read only for `v1`, and
    both files at once are refused. The base is history: it may hold the
    old file saying `v2` (until the move lands), or both files through a
    commit that skipped the check, so neither rule applies there, and the
    higher of the two counts."""
    text, old_text = tree.read(STANDARD_FILE), tree.read(OLD_STANDARD_FILE)
    if not history and text is not None and old_text is not None:
        raise ConfigError(f"{OLD_STANDARD_FILE} and {STANDARD_FILE} are both here; keep {STANDARD_FILE} alone")
    found, doc = [], None
    if text is not None:
        doc = parse_toml(text, tree.where(STANDARD_FILE))
        found.append(standard_in_doc(doc, tree.where(STANDARD_FILE)))
    if old_text is not None:
        old = standard_in_line(old_text, tree.where(OLD_STANDARD_FILE))
        if not history and old != "v1":
            raise ConfigError(f"{OLD_STANDARD_FILE} says {old}; move it to {STANDARD_FILE} as [standard] "
                              f"url = \"{STANDARD_URL}\", version = \"{old}\"")
        found.append(old)
    return (max(found, key=rank) if found else None), (None if doc is None else doc.get("platform"))


def parse_toml(text, name):
    try:
        return tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise Unreadable(f"{name}: not valid TOML: {e}")


def verify_base(root, base):
    """Refuse a --base that is not a commit here: an unfetched ref, a typo, a
    shallow clone or a --root that is not a repository must never read as
    "a base with no standard"."""
    try:
        git("rev-parse", "--verify", "--quiet", f"{base}^{{commit}}", cwd=root)
    except ConfigError:
        raise ConfigError(f"--base {base}: not a commit in {root}; fetch it, or pass a ref that exists")


def no_downgrade(base, found, platform):
    """Adopted standards only go up: refuse a head that names a lower standard
    than its base, or none where the base names one. The same for
    `[platform]`: a toolchain the base names may not disappear. `base` is a
    Commit; `found` and `platform` are the head's. The notes for the summary
    come back.

    A base whose pointer is there but Unreadable would refuse every pull
    request, the one that repairs it too. Then the head must name ENFORCED,
    the highest standard this script knows, so it cannot be lower than
    whatever the base meant. The base's [platform] is still compared when
    its file is TOML (only its [standard] is bad); it is not compared only
    when the file cannot be parsed at all."""
    try:
        was, before = pointer(base, history=True)
    except Unreadable as e:
        if found != ENFORCED:
            raise ConfigError(f"{e}; with the base's standard unreadable, this head must name {ENFORCED}, "
                              f"the highest standard this script knows")
        parsed, before = base_platform(base)
        platform_kept(base, before, platform)
        compared = ("its [platform] is compared" if parsed
                    else "the base's [platform] is not compared, since its file cannot be parsed")
        return [f"The base's standard cannot be read ({e}). This head names {ENFORCED}, the highest this "
                f"script knows, so it is no downgrade; {compared}."]
    if was is not None and (found is None or rank(found) < rank(was)):
        raise ConfigError(f"the standard goes down from {was} at {base.ref} to {found or 'none'}; "
                          f"an adopted standard only goes up")
    platform_kept(base, before, platform)
    return []


def base_platform(base):
    """(whether the base's tehas.toml parsed as TOML, its [platform]), for a
    base whose [standard] is bad."""
    try:
        text = base.read(STANDARD_FILE)
        if text is None:
            return False, None
        return True, parse_toml(text, base.where(STANDARD_FILE)).get("platform")
    except ConfigError:
        return False, None


def platform_kept(base, before, platform):
    """A toolchain the base declares may not disappear at the head."""
    after = platform if isinstance(platform, dict) else {}
    if before is not None and not isinstance(before, dict):
        # The base declared its toolchains in a shape that has no keys to
        # compare; the head keeps a [platform] that names them.
        if not after:
            raise ConfigError(f"{base.where(STANDARD_FILE)}: [platform] is there (not a table) and this head has "
                              f"none; a declared toolchain only moves forward: name them in a [platform] table")
    elif before:
        gone = sorted(set(before) - set(after))
        if gone:
            raise ConfigError(f"[platform] loses {', '.join(gone)} against {base.ref}; "
                              f"a declared toolchain only moves forward")


def rank(version):
    return tuple(int(n) for n in version[1:].split("."))


def standard_in_doc(doc, name):
    table = doc.get("standard")
    if table is None:
        # Fail closed: a file that exists names its standard. A near miss
        # ([Standard], [standards], a top-level version) must not read as
        # "no standard" and switch the checks off.
        raise Unreadable(f"{name}: no [standard] table; write [standard] url = \"{STANDARD_URL}\", "
                         f"version = \"{ENFORCED}\" (a repository that names no standard has no {STANDARD_FILE})")
    if not isinstance(table, dict):
        raise Unreadable(f"{name}: [standard] must be a table")
    extra = sorted(set(table) - STANDARD_KEYS)
    if extra:
        raise Unreadable(f"{name}: [standard] has keys this script does not know: {', '.join(extra)}")
    if "version" not in table:
        raise Unreadable(f"{name}: [standard] has no version; write one of {known()}")
    version = checked_version(table["version"], name)
    url = table.get("url")
    if url is None:
        raise Unreadable(f"{name}: [standard] has no url; write url = \"{STANDARD_URL}\"")
    if url != STANDARD_URL:
        raise Unreadable(f"{name}: [standard] url is {url!r}; this script checks {STANDARD_URL} only, "
                         f"and another standard brings its own script")
    return version


def standard_in_line(text, name):
    """The first line that is not empty and not a `#` comment."""
    for line in text.splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            return checked_version(line, name)
    raise Unreadable(f"{name}: empty; write one of {known()}")


def known():
    return ", ".join(sorted(STANDARDS))


def checked_version(value, name):
    if not isinstance(value, str):
        raise Unreadable(f"{name}: the version must be a string such as \"v2\"")
    if re.fullmatch(r"v[1-9][0-9]*(\.[0-9]+)?", value) and value not in STANDARDS:
        # Not Unreadable: a base at a later standard is no repair to make,
        # it asks for that standard's script.
        raise ConfigError(f"{name}: standard {value} is newer than this script, "
                          f"which knows {known()}; take the architecture.py that defines {value}")
    if value not in STANDARDS:
        raise Unreadable(f"{name}: {value!r} is not a standard; write one of {known()}")
    return value


def check_platform(table):
    """The head's `[platform]` (None when it has none), checked: the
    toolchains the repository builds or checks with, each at a major this
    standard allows."""
    if table is None:
        return
    if not isinstance(table, dict):
        raise ConfigError(f"{STANDARD_FILE}: [platform] must be a table")
    for key in sorted(table):
        allowed = PLATFORM_ALLOWED.get(key)
        if allowed is None:
            raise ConfigError(f"{STANDARD_FILE}: [platform] has a key this script does not know: {key} "
                              f"(it knows {', '.join(sorted(PLATFORM_ALLOWED))})")
        value = table[key]
        if not isinstance(value, str) or value not in allowed:
            raise ConfigError(f"{STANDARD_FILE}: [platform] {key} = {value!r}; this standard allows "
                              f"{', '.join(sorted(allowed))}")


def skipped(found, summary, notes=()):
    said = f"is {found}" if found else f"is not set (no {STANDARD_FILE})"
    out = ["## Architecture test", ""] + [n + "\n" for n in notes]
    out += [f"Skipped: this repository's standard {said}. The checks are enforced at {ENFORCED}. "
            f"Once the first refactoring runs are done, write this table in `{STANDARD_FILE}`:",
            "", "```toml", "[standard]", f"url = \"{STANDARD_URL}\"", f"version = \"{ENFORCED}\"", "```",
            "", "`--force` shows the counts now."]
    return write(out, summary, 0)


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--config", default="architecture.toml")
    ap.add_argument("--base", help="the pull request's base, as a git ref")
    ap.add_argument("--summary", default=os.environ.get("GITHUB_STEP_SUMMARY"), help="a file to append the summary to")
    ap.add_argument("--root", default=".")
    ap.add_argument("--force", action="store_true", help=f"run the checks below standard {ENFORCED}")
    a = ap.parse_args(argv)
    try:
        found, platform = pointer(WorkTree(a.root))
        check_platform(platform)
        notes = []
        if a.base:
            verify_base(a.root, a.base)
            notes = no_downgrade(Commit(a.root, a.base), found, platform)
        if found != ENFORCED and not a.force:
            return skipped(found, a.summary, notes)
        return run(a.root, a.config, a.base, a.summary, notes)
    except (ConfigError, OSError) as e:
        print(f"architecture: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
