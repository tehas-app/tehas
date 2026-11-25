#!/usr/bin/env python3
"""The architecture test: the mechanical checks of the architecture method.

    scripts/architecture.py [--config FILE] [--base REF] [--summary FILE] [--force]

The method is one for every codebase of the tehas system (tehas-docs,
wiki/decisions/architecture-method.md, section 5). This script is the same
in every repository; what it checks is the repository's own
`architecture.toml`.

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
  (`pattern`, `files`, `allowed`, `mode`, `start`, `end`, `comment`), whose
  `why` must also not be empty, even at a `max` of 0. A check that is in
  the base must still be here.

A `[[report]]` counts and lists without failing, for what is judged in
review rather than checked (public items per module, for example). With
`--base`, a report in the base must still be here, and one whose scope
changed (its `top` included) must come with a new, non-empty `why`.

Nothing here knows a language or a repository. Modes: `lines` (default)
counts each line that matches. `spans` counts each span of lines that
opens on a line matching `start` and closes on the first line matching
`end` (a function's signature, say: from `fn` to `{` in Rust, from
`function` to `{` in JavaScript), once, if it matches anywhere in it; the
text from the `end` match on is left out. Lines whose first characters are
the check's `comment` prefix are skipped.

The repository says which standard it follows in `.tehas/repository.toml`,
table `[standard]`, key `version` (before v2 the one line of
`.tehas/coding-standard`, read only for `v1`): `v1` (the code as it was,
before its first refactoring runs) or `v2` (the architecture method as the
wiki page defines it; a
later standard, v3 or more, comes with the version of this script that
enforces it, and this one refuses it). The checks are
enforced only at `v2`. At `v1`, or with no such file, the test says it is
skipped and passes; `--force` runs it anyway, to see the counts while the
refactoring runs. Any other value is refused.

Exit codes: 0 every check holds or the test is skipped, 1 a check fails, 2 the configuration or
the repository cannot be read. Needs python3 (3.11 or later) and git. No
network.
"""

import argparse
import os
import re
import subprocess
import sys
import tomllib

CHECK_KEYS = {"id", "rule", "what", "pattern", "files", "allowed", "max", "why", "mode", "comment", "start", "end"}
CHECK_REQUIRED = {"id", "rule", "what", "pattern", "files", "max"}
# Every key but these decides what a check counts, so changing it moves
# the count as surely as raising max does.
NOT_SCOPE = {"id", "rule", "what", "max", "why"}
SCOPE_DEFAULTS = {"allowed": [], "mode": "lines", "top": 10}
REPORT_KEYS = {"id", "rule", "what", "pattern", "files", "allowed", "top", "mode", "comment", "start", "end", "why"}
REPORT_REQUIRED = {"id", "rule", "what", "pattern", "files"}
TOP_KEYS = {"version", "method", "review", "check", "report"}
STANDARD_FILE = ".tehas/repository.toml"  # its [standard] table; other tables have other owners
OLD_STANDARD_FILE = ".tehas/coding-standard"  # the pointer's name before v2; read only for v1
STANDARD_KEYS = {"version"}
STANDARDS = {"v1", "v2"}
ENFORCED = "v2"
MODES = {"lines", "spans"}
RULE = re.compile(r"R([1-9]|10)")
SHOWN = 20


class ConfigError(Exception):
    pass


def git(*args, cwd="."):
    r = subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True)
    if r.returncode != 0:
        raise ConfigError(f"git {' '.join(args)}: {r.stderr.strip() or 'failed'}")
    return r.stdout


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
            for k in ("files", "allowed"):
                v = c.get(k, [])
                if not (isinstance(v, list) and all(isinstance(x, str) and x for x in v)):
                    problems.append(f"{name}: {k} must be a list of path globs")
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


def count(entry, files, read):
    """The places `entry` matches outside its allowed files."""
    pattern = re.compile(entry["pattern"])
    include = [glob_re(g) for g in entry["files"]]
    allowed = [glob_re(g) for g in entry.get("allowed", [])]
    hits = []
    for path in files:
        if not matches_any(path, include) or matches_any(path, allowed):
            continue
        text = read(path)
        if text is None:
            continue
        for n, line in units(text, entry):
            if pattern.search(line):
                hits.append((path, n, line.strip()))
    return hits


def tracked(root):
    return [p for p in git("ls-files", "-z", cwd=root).split("\0") if p]


def reader(root):
    def read(path):
        try:
            with open(os.path.join(root, path), encoding="utf-8") as f:
                return f.read()
        except (UnicodeDecodeError, FileNotFoundError, IsADirectoryError):
            return None
    return read


def run(root, config_path, base, summary):
    with open(os.path.join(root, config_path), encoding="utf-8") as f:
        text = f.read()
    doc = parse(text, config_path)
    files = tracked(root)
    read = reader(root)
    failures, rows, notes = [], [], []

    base_checks = None
    base_reports = None
    if base:
        try:
            base_text = git("show", f"{base}:{config_path}", cwd=root)
        except ConfigError:
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
    out += notes
    out += ["| Check | Rule | Base max | Max | Now | |", "| --- | --- | --- | --- | --- | --- |"]
    out += [f"| `{a}` | {b} | {c} | {d} | {e} | {f} |" for a, b, c, d, e, f in rows]
    for r, total, top in reports:
        out += ["", f"**{r['id']}** ({r['rule']}, reported, not checked): {r['what']}. {total} in all."]
        out += [f"- `{p}`: {n}" for p, n in top]
    if doc.get("review"):
        out += ["", f"Review the pull request against the questions in `{doc['review']}`."]
    if failures:
        out += ["", "### What fails", "", "```"] + failures + ["```"]
    text = "\n".join(out) + "\n"
    print(text)
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(text)
    return 1 if failures else 0


def standard(root):
    """The repository's standard: `v1`, `v2`, or None when it names none.
    Read from `.tehas/repository.toml`, table `[standard]`, key `version`.
    This script owns that table only: an unknown key in it is refused, other
    tables are left to their owners. The old one-line `.tehas/coding-standard`
    is read only for `v1`. Both at once, or either as a symlink, is refused."""
    new_path = os.path.join(root, STANDARD_FILE)
    old_path = os.path.join(root, OLD_STANDARD_FILE)
    for path, name in ((new_path, STANDARD_FILE), (old_path, OLD_STANDARD_FILE)):
        if os.path.islink(path):
            raise ConfigError(f"{name}: a symlink; write the file itself")
    new, old = os.path.lexists(new_path), os.path.lexists(old_path)
    if new and old:
        raise ConfigError(f"{OLD_STANDARD_FILE} and {STANDARD_FILE} are both here; keep {STANDARD_FILE} alone")
    if old:
        found = standard_in_line(read_text(old_path, OLD_STANDARD_FILE), OLD_STANDARD_FILE)
        if found != "v1":
            raise ConfigError(f"{OLD_STANDARD_FILE} says {found}; move it to {STANDARD_FILE} as "
                              f"[standard] version = \"{found}\"")
        return found
    if not new:
        return None
    return standard_in_toml(read_text(new_path, STANDARD_FILE), STANDARD_FILE)


def verify_base(root, base):
    """Refuse a --base that is not a commit here: an unfetched ref, a typo, a
    shallow clone or a --root that is not a repository must never read as
    "a base with no standard"."""
    try:
        git("rev-parse", "--verify", "--quiet", f"{base}^{{commit}}", cwd=root)
    except ConfigError:
        raise ConfigError(f"--base {base}: not a commit in {root}; fetch it, or pass a ref that exists")


def base_standard(root, base):
    """The standard the base names, by either file name, or None when the base
    has neither. The base is history: it may hold the old file saying `v2`
    (main does, until the move lands), so the old-name rule of standard() is
    deliberately not applied here. With both files at the base, the new one
    counts."""
    for name, parse_ in ((STANDARD_FILE, standard_in_toml), (OLD_STANDARD_FILE, standard_in_line)):
        entry = git("ls-tree", base, "--", name, cwd=root).strip()
        if not entry:
            continue
        mode = entry.split()[0]
        if mode not in ("100644", "100755"):
            raise ConfigError(f"{base}:{name}: not a regular file (mode {mode})")
        return parse_(git("show", f"{base}:{name}", cwd=root), f"{base}:{name}")
    return None


def no_downgrade(root, base, found):
    """Adopted standards only go up: refuse a head that names a lower standard
    than its base, or none where the base names one."""
    was = base_standard(root, base)
    if was is not None and (found is None or rank(found) < rank(was)):
        raise ConfigError(f"the standard goes down from {was} at {base} to {found or 'none'}; "
                          f"an adopted standard only goes up")


def rank(version):
    return tuple(int(n) for n in version[1:].split("."))


def read_text(path, name):
    try:
        with open(path, encoding="utf-8") as f:
            return f.read()
    except UnicodeDecodeError:
        raise ConfigError(f"{name}: not UTF-8")
    except IsADirectoryError:
        raise ConfigError(f"{name}: a directory, not a file")


def standard_in_toml(text, name):
    try:
        doc = tomllib.loads(text)
    except tomllib.TOMLDecodeError as e:
        raise ConfigError(f"{name}: not valid TOML: {e}")
    table = doc.get("standard")
    if table is None:
        # Fail closed: a file that exists names its standard. A near miss
        # ([Standard], [standards], a top-level version) must not read as
        # "no standard" and switch the checks off.
        raise ConfigError(f"{name}: no [standard] table; write [standard] version = \"{ENFORCED}\" "
                          f"(a repository that names no standard has no {STANDARD_FILE})")
    if not isinstance(table, dict):
        raise ConfigError(f"{name}: [standard] must be a table")
    extra = sorted(set(table) - STANDARD_KEYS)
    if extra:
        raise ConfigError(f"{name}: [standard] has keys this script does not know: {', '.join(extra)}")
    if "version" not in table:
        raise ConfigError(f"{name}: [standard] has no version; write one of {known()}")
    return checked_version(table["version"], name)


def standard_in_line(text, name):
    """The first line that is not empty and not a `#` comment."""
    for line in text.splitlines():
        line = line.strip()
        if line and not line.startswith("#"):
            return checked_version(line, name)
    raise ConfigError(f"{name}: empty; write one of {known()}")


def known():
    return ", ".join(sorted(STANDARDS))


def checked_version(value, name):
    if not isinstance(value, str):
        raise ConfigError(f"{name}: the version must be a string such as \"v2\"")
    if re.fullmatch(r"v[1-9][0-9]*(\.[0-9]+)?", value) and value not in STANDARDS:
        raise ConfigError(f"{name}: standard {value} is newer than this script, "
                          f"which knows {known()}; take the architecture.py that defines {value}")
    if value not in STANDARDS:
        raise ConfigError(f"{name}: {value!r} is not a standard; write one of {known()}")
    return value


PLATFORM_ALLOWED = {"rust": {"1.x"}, "typescript": {"7.x"}}


def platform(root):
    """The `[platform]` table of `.tehas/repository.toml`, checked: the
    toolchains the repository builds with, each at a major this standard
    allows. This script owns the table for now; an unknown key or a value
    the standard does not allow is refused. None with no file or no table."""
    path = os.path.join(root, STANDARD_FILE)
    if not os.path.lexists(path) or os.path.islink(path):
        return None
    doc = tomllib.loads(read_text(path, STANDARD_FILE))
    table = doc.get("platform")
    if table is None:
        return None
    if not isinstance(table, dict):
        raise ConfigError(f"{STANDARD_FILE}: [platform] must be a table")
    for key in sorted(table):
        allowed = PLATFORM_ALLOWED.get(key)
        if allowed is None:
            raise ConfigError(f"{STANDARD_FILE}: [platform] has a key this script does not know: {key} "
                              f"(it knows {', '.join(sorted(PLATFORM_ALLOWED))})")
        if table[key] not in allowed:
            raise ConfigError(f"{STANDARD_FILE}: [platform] {key} = {table[key]!r}; this standard allows "
                              f"{', '.join(sorted(allowed))}")
    return table


def skipped(found, summary):
    said = f"is {found}" if found else f"is not set (no [standard] version in {STANDARD_FILE})"
    text = (f"## Architecture test\n\nSkipped: this repository's standard {said}. "
            f"The checks are enforced at {ENFORCED}; write `[standard] version = \"{ENFORCED}\"` in `{STANDARD_FILE}` "
            f"once the first refactoring runs are done. `--force` shows the counts now.\n")
    print(text)
    if summary:
        with open(summary, "a", encoding="utf-8") as f:
            f.write(text)
    return 0


def main(argv=None):
    ap = argparse.ArgumentParser(description=__doc__.split("\n\n")[0])
    ap.add_argument("--config", default="architecture.toml")
    ap.add_argument("--base", help="the pull request's base, as a git ref")
    ap.add_argument("--summary", default=os.environ.get("GITHUB_STEP_SUMMARY"), help="a file to append the summary to")
    ap.add_argument("--root", default=".")
    ap.add_argument("--force", action="store_true", help=f"run the checks below standard {ENFORCED}")
    a = ap.parse_args(argv)
    try:
        found = standard(a.root)
        platform(a.root)
        if a.base:
            verify_base(a.root, a.base)
            no_downgrade(a.root, a.base, found)
        if found != ENFORCED and not a.force:
            return skipped(found, a.summary)
        return run(a.root, a.config, a.base, a.summary)
    except (ConfigError, OSError) as e:
        print(f"architecture: {e}", file=sys.stderr)
        return 2


if __name__ == "__main__":
    sys.exit(main())
