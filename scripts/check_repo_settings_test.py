#!/usr/bin/env python3
# Copyright 2026 Jani Tarvainen
# SPDX-License-Identifier: Apache-2.0
"""Tests of scripts/check-repo-settings.py, on fixture answers. No network.
Run: python3 -I -m unittest discover -s scripts -p '*_test.py'"""

import json
import os
import shutil
import sys
import tempfile
import types
import unittest

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import importlib.util

_spec = importlib.util.spec_from_file_location(
    "check_repo_settings",
    os.path.join(os.path.dirname(os.path.abspath(__file__)), "check-repo-settings.py"),
)
settings_mod = importlib.util.module_from_spec(_spec)
_spec.loader.exec_module(settings_mod)

OWNER_REPO = "octo/widgets"
GATE = settings_mod.GATE_PATHS


def main_ruleset(**over):
    r = {
        "name": "main",
        "target": "branch",
        "enforcement": "active",
        "conditions": {"ref_name": {"include": ["~DEFAULT_BRANCH"], "exclude": []}},
        "rules": [
            {"type": "deletion"},
            {"type": "non_fast_forward"},
            {"type": "pull_request",
             "parameters": {"required_approving_review_count": 1,
                            "require_code_owner_review": True}},
            {
                "type": "required_status_checks",
                "parameters": {
                    "required_status_checks": [
                        {"context": "verify"},
                        {"context": "repo-settings"},
                    ]
                },
            },
        ],
        "bypass_actors": [],
    }
    r.update(over)
    return r


class World:
    """A fixture directory and a CODEOWNERS root, built once per case."""

    def __init__(self, tmp, main=None, codeowners=None, account=None):
        tmp = tempfile.mkdtemp(prefix="case-", dir=tmp)
        self.fixtures = os.path.join(tmp, "api")
        self.root = os.path.join(tmp, "checkout")
        os.makedirs(self.fixtures)
        os.makedirs(os.path.join(self.root, ".github"))
        os.makedirs(os.path.join(self.root, "api"))
        self.write("repos_octo_widgets.json", {"default_branch": "main"})
        self.write("repos_octo_widgets_rulesets.json", [{"id": 1, "name": "main", "target": "branch"}])
        self.write("repos_octo_widgets_rulesets_1.json", main or main_ruleset())
        lines = ["* @octo"]
        for p in GATE:
            lines.append(f"{p} @octo")
        with open(os.path.join(self.root, ".github", "CODEOWNERS"), "w", encoding="utf-8") as f:
            f.write("\n".join(lines if codeowners is None else codeowners) + "\n")
        if account:
            with open(os.path.join(self.root, "api", "plugins-review-account"), "w", encoding="utf-8") as f:
                json.dump(account, f)

    def write(self, name, obj):
        with open(os.path.join(self.fixtures, name), "w", encoding="utf-8") as f:
            json.dump(obj, f)

    def read(self, name):
        with open(os.path.join(self.fixtures, name), encoding="utf-8") as f:
            return json.load(f)

    def fails(self):
        src = settings_mod.Source("unused", self.fixtures, None)
        args = types.SimpleNamespace(repo=OWNER_REPO)
        return settings_mod.check(args, src, self.root)


class CheckRepoSettings(unittest.TestCase):
    def setUp(self):
        self.tmp = tempfile.mkdtemp(prefix="repo-settings-")
        self.addCleanup(shutil.rmtree, self.tmp, True)

    def world(self, **over):
        return World(self.tmp, **over)

    def test_all_hold(self):
        self.assertEqual(self.world().fails(), [])

    def test_main_disabled(self):
        w = self.world(main=main_ruleset(enforcement="disabled"))
        self.assertTrue(any("whole-branch" in f for f in w.fails()), w.fails())
        w = self.world(main=main_ruleset(rules=[]))
        self.assertTrue(any("deletion is not blocked" in f for f in w.fails()), w.fails())

    def test_bypass_actor(self):
        w = self.world(main=main_ruleset(bypass_actors=[{"actor_id": 1, "actor_type": "Team"}]))
        self.assertTrue(any("bypass actors" in f for f in w.fails()), w.fails())

    def test_no_pull_request_rule(self):
        r = main_ruleset()
        r["rules"] = [x for x in r["rules"] if x["type"] != "pull_request"]
        w = self.world(main=r)
        self.assertTrue(any("pull request is not required" in f for f in w.fails()), w.fails())

    def test_check_not_required(self):
        r = main_ruleset()
        for x in r["rules"]:
            if x["type"] == "required_status_checks":
                x["parameters"]["required_status_checks"] = [{"context": "verify"}]
        w = self.world(main=r)
        self.assertTrue(any("repo-settings" in f for f in w.fails()), w.fails())

    def test_code_owner_not_required(self):
        r = main_ruleset()
        for x in r["rules"]:
            if x["type"] == "pull_request":
                x["parameters"]["require_code_owner_review"] = False
        w = self.world(main=r)
        self.assertTrue(any("code owner's review" in f for f in w.fails()), w.fails())

    def test_no_approving_review(self):
        r = main_ruleset()
        for x in r["rules"]:
            if x["type"] == "pull_request":
                x["parameters"]["required_approving_review_count"] = 0
        w = self.world(main=r)
        self.assertTrue(any("approving review" in f for f in w.fails()), w.fails())

    def test_gate_path_without_owner(self):
        lines = ["* @octo"] + [f"{p} @octo" for p in GATE[:-1]]
        w = self.world(codeowners=lines)
        self.assertTrue(any(GATE[-1] in f and "no code owner" in f for f in w.fails()), w.fails())

    def test_unreadable_fails_closed(self):
        w = self.world()
        os.remove(os.path.join(w.fixtures, "repos_octo_widgets_rulesets.json"))
        self.assertTrue(any("cannot be read" in f for f in w.fails()), w.fails())

    def test_reviewing_account_must_own_the_gate(self):
        w = self.world(account={"login": "octo-review", "id": 42})
        fails = w.fails()
        self.assertTrue(all("reviewing account" in f for f in fails), fails)
        self.assertEqual(len(fails), len(GATE))
        lines = ["* @octo"] + [f"{p} @octo @octo-review" for p in GATE]
        w2 = self.world(account={"login": "octo-review", "id": 42}, codeowners=lines)
        self.assertEqual(w2.fails(), [])

    def test_reviewing_account_malformed(self):
        w = self.world(account={"login": "octo-review"})
        self.assertTrue(any("malformed" in f for f in w.fails()), w.fails())


if __name__ == "__main__":
    unittest.main()
