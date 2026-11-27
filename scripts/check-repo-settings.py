#!/usr/bin/env python3
"""repo-settings: read the repository's own branch rules back and fail closed.

What the plugin spec 2.0.0 part 9 (the gate files) and tehas-standard v3
§11 (the host protects its own default branch) ask of this repository is
rules, not habit. This check reads them from the code host and fails when
one is off, so protection lost reads as a failed build, not as silence.
It checks:

- the default branch has an active ruleset that blocks deletion and
  force-push, requires a pull request, names no bypass actor, and
  requires the gate's checks;
- an active ruleset gives every gate path its code-owner review, with no
  bypass actor;
- every gate path has a code owner in .github/CODEOWNERS, and once
  api/plugins-review-account names the reviewing account, that account is
  among them;
- api/plugins-review-account, when it exists, holds a login and an
  immutable account id.

Usage: check-repo-settings.py [--repo owner/name] [--base-url URL]
                              [--fixtures DIR]
  --repo      default: the GITHUB_REPOSITORY environment.
  --base-url  the code host's API, default https://api.github.com.
  --fixtures  read each answer from DIR/<path with / as _>.json instead of
              the network; for the tests, never on a host.

A token from GITHUB_TOKEN is sent when set. Exit 0 when every check holds,
1 when one fails or an answer cannot be read: a rule nobody can read has
not been shown to hold.
"""

import argparse
import json
import os
import sys
import urllib.error
import urllib.request

GATE_PATHS = [
    "api/plugins-blessed*",
    "api/plugins-signers*",
    "api/plugins-default*",
    "api/plugins-review-account*",
    ".github/CODEOWNERS",
    ".github/workflows/release.yml",
    ".github/workflows/repo-settings.yml",
    "scripts/check-repo-settings.py",
]

# The checks a pull request into the default branch must wait for.
REQUIRED_CHECKS = ["verify", "repo-settings"]

PAGE_SIZE = 100


class Unreadable(Exception):
    """An answer the code host could not give, so a rule is not shown to hold."""


class Source:
    """Reads the code host, or the fixtures of a test run."""

    def __init__(self, base_url, fixtures, token):
        self.base_url = base_url.rstrip("/")
        self.fixtures = fixtures
        self.token = token

    def get(self, path, params=None):
        if self.fixtures:
            name = path.strip("/").replace("/", "_") + ".json"
            try:
                with open(os.path.join(self.fixtures, name), encoding="utf-8") as f:
                    return json.load(f)
            except (OSError, ValueError) as e:
                raise Unreadable(f"{name}: {e}") from e
        url = self.base_url + path
        if params:
            url += "?" + "&".join(f"{k}={v}" for k, v in sorted(params.items()))
        req = urllib.request.Request(url)
        req.add_header("Accept", "application/vnd.github+json")
        if self.token:
            req.add_header("Authorization", f"Bearer {self.token}")
        try:
            with urllib.request.urlopen(req, timeout=30) as r:
                return json.load(r)
        except (urllib.error.URLError, urllib.error.HTTPError, ValueError, TimeoutError) as e:
            raise Unreadable(f"{url}: {e}") from e


def paths_include(ruleset):
    """The path patterns a ruleset limits itself to, as a list or None."""
    paths = (ruleset.get("conditions") or {}).get("paths")
    if paths is None:
        return None
    if isinstance(paths, dict):
        return paths.get("include") or None
    return paths or None


def refs_include(ruleset):
    return ((ruleset.get("conditions") or {}).get("ref_name") or {}).get("include") or []


def rule_of(ruleset, kind):
    for r in ruleset.get("rules") or []:
        if r.get("type") == kind:
            return r
    return None


def codeowners_entries(path):
    """The CODEOWNERS lines as (pattern, owners) pairs, in file order."""
    entries = []
    try:
        with open(path, encoding="utf-8") as f:
            for line in f:
                line = line.split("#", 1)[0].strip()
                if not line:
                    continue
                parts = line.split()
                if len(parts) >= 2:
                    entries.append((parts[0], parts[1:]))
    except OSError as e:
        raise Unreadable(f"{path}: {e}") from e
    return entries


def check(settings, source, root):
    """Return every failed check as one line of text."""
    fails = []
    repo = settings.repo
    try:
        info = source.get(f"/repos/{repo}")
        branch = info["default_branch"]
        branch_refs = [f"refs/heads/{branch}", "~DEFAULT_BRANCH"]

        listed = source.get(f"/repos/{repo}/rulesets", {"per_page": PAGE_SIZE})
        if not isinstance(listed, list):
            raise Unreadable("the ruleset list is not a list")
        if len(listed) == PAGE_SIZE:
            raise Unreadable("a full page of rulesets: the page beyond is unread, so the rules are not shown to hold")
        rulesets = []
        for item in listed:
            if item.get("target") != "branch":
                continue
            full = source.get(f"/repos/{repo}/rulesets/{item['id']}")
            full.setdefault("name", item.get("name"))
            rulesets.append(full)
    except (Unreadable, KeyError, TypeError) as e:
        return [f"the rules cannot be read: {e}"]

    mine = [r for r in rulesets if any(ref in branch_refs for ref in refs_include(r))]
    if not mine:
        fails.append(f"no branch ruleset covers the default branch {branch!r}")

    def bypass_free(r, who):
        if r.get("bypass_actors"):
            fails.append(f"{who}: bypass actors: {r['bypass_actors']}")

    active = [r for r in mine if r.get("enforcement") == "active"]
    for r in mine:
        bypass_free(r, f"ruleset {r.get('name')!r}")
    # The whole-branch rules must sit in a ruleset without a path condition:
    # a ruleset over the gate paths alone says nothing about a pull request
    # that touches anything else.
    wide = [r for r in active if paths_include(r) is None]
    if mine and not wide:
        fails.append(f"no active whole-branch ruleset on the default branch (found: {[r.get('name') for r in mine]})")

    if wide:
        union = set()
        checks_seen = set()
        pr_params = {}
        for r in wide:
            union.update(x.get("type") for x in r.get("rules") or [])
            for c in (rule_of(r, "required_status_checks") or {}).get("parameters", {}).get("required_status_checks", []) or []:
                checks_seen.add(c.get("context"))
            p = (rule_of(r, "pull_request") or {}).get("parameters") or {}
            # Merge two rulesets' pull_request parameters as the code host
            # does: what any one of them demands, the branch must hold.
            for k, v in p.items():
                if k == "required_approving_review_count":
                    pr_params[k] = max(pr_params.get(k, 0), v)
                elif v is True:
                    pr_params[k] = True
        for kind, why in [("deletion", "branch deletion is not blocked"),
                          ("non_fast_forward", "force-push is not blocked"),
                          ("pull_request", "a pull request is not required before a merge")]:
            if kind not in union:
                fails.append(f"the default branch: {why}")
        for name in REQUIRED_CHECKS:
            if name not in checks_seen:
                fails.append(f"the default branch: the check {name!r} is not required")
        if not pr_params.get("require_code_owner_review"):
            fails.append("the default branch: the code owner's review is not required")
        if pr_params.get("required_approving_review_count", 0) < 1:
            fails.append("the default branch: no approving review is required")

    try:
        entries = codeowners_entries(os.path.join(root, ".github", "CODEOWNERS"))
    except Unreadable as e:
        return fails + [str(e)]

    for pattern in GATE_PATHS:
        owners = None
        for pat, own in entries:
            if pat == pattern:
                owners = own
        if not owners:
            fails.append(f"the gate path {pattern!r} has no code owner in CODEOWNERS")

    account_file = os.path.join(root, "api", "plugins-review-account")
    if os.path.exists(account_file):
        try:
            with open(account_file, encoding="utf-8") as f:
                account = json.load(f)
            login, acc_id = account["login"], account["id"]
        except (OSError, ValueError, KeyError) as e:
            fails.append(f"api/plugins-review-account is unread or malformed: {e}")
        else:
            if not isinstance(acc_id, int):
                fails.append("api/plugins-review-account: the id is not a number")
            for pattern in GATE_PATHS:
                owners = None
                for pat, own in entries:
                    if pat == pattern:
                        owners = own
                if owners and f"@{login}" not in owners:
                    fails.append(f"the gate path {pattern!r} lacks the reviewing account @{login}")
    return fails


def main(argv=None):
    ap = argparse.ArgumentParser()
    ap.add_argument("--repo", default=os.environ.get("GITHUB_REPOSITORY", ""))
    ap.add_argument("--base-url", default="https://api.github.com")
    ap.add_argument("--fixtures")
    args = ap.parse_args(argv)
    if not args.repo or "/" not in args.repo:
        print("fail  no repository named: pass --repo owner/name or set GITHUB_REPOSITORY")
        return 1
    source = Source(args.base_url, args.fixtures, os.environ.get("GITHUB_TOKEN"))
    fails = check(args, source, ".")
    for line in fails:
        print(f"fail  {line}")
    if fails:
        return 1
    print(f"ok    repo-settings: the default branch and {len(GATE_PATHS)} gate paths hold their rules")
    return 0


if __name__ == "__main__":
    sys.exit(main())
