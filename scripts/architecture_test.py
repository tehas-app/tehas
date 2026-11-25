#!/usr/bin/env python3
"""Tests of scripts/architecture.py, on throwaway git repositories. No
network. Run: python3 -I scripts/architecture_test.py"""

import io
import os
import subprocess
import sys
import tempfile
import unittest
from contextlib import redirect_stderr, redirect_stdout

sys.dont_write_bytecode = True
sys.path.insert(0, os.path.dirname(os.path.abspath(__file__)))
import architecture  # noqa: E402

CONFIG = '''version = 1

[[check]]
id = "routes"
rule = "R1"
what = "routes only in the adapter"
pattern = '/api/'
files = ["src/**/*.rs"]
allowed = ["src/adapter.rs"]
comment = "//"
max = {routes}
{why}

[[check]]
id = "untyped"
rule = "R4"
what = "no Value in a signature"
mode = "spans"
start = '\\bfn\\s+\\w+'
end = '[{{;]'
pattern = '\\bValue\\b'
files = ["src/**/*.rs"]
max = {untyped}
why = "the page's handler, until step 3"

[[report]]
id = "pub"
rule = "R5"
what = "public items"
pattern = '^pub '
files = ["src/**"]
'''


def config(routes=1, untyped=1, why="the page, until step 4"):
    return CONFIG.format(routes=routes, untyped=untyped, why=f'why = "{why}"' if why else "")


class Repo:
    def __init__(self):
        self.tmp = tempfile.TemporaryDirectory()
        unittest.addModuleCleanup(self.tmp.cleanup)
        self.root = self.tmp.name
        self.git("init", "-q", "-b", "main")
        self.git("config", "user.email", "test@example.invalid")
        self.git("config", "user.name", "test")

    def git(self, *args):
        subprocess.run(["git", *args], cwd=self.root, check=True, capture_output=True)

    def write(self, path, text):
        full = os.path.join(self.root, path)
        os.makedirs(os.path.dirname(full), exist_ok=True)
        with open(full, "w") as f:
            f.write(text)
        self.git("add", path)

    def commit(self):
        self.git("commit", "-q", "-m", "x")

    def run(self, *args):
        out, err = io.StringIO(), io.StringIO()
        with redirect_stdout(out), redirect_stderr(err):
            code = architecture.main(["--root", self.root, "--summary", os.devnull, *args])
        return code, out.getvalue() + err.getvalue()


def pointer(standard):
    """`.tehas/repository.toml` naming `standard`, or the text as given when it
    is already TOML (it holds a table)."""
    if "[" in standard:
        return standard + "\n"
    return f'[standard]\nversion = "{standard}"\n'


def base_repo(standard="v2", **kw):
    r = Repo()
    if standard:
        r.write(".tehas/repository.toml", pointer(standard))
    r.write("architecture.toml", config(**kw))
    r.write("src/adapter.rs", 'fn get() { call("/api/a"); call("/api/b") }\n')
    r.write("src/web.rs", '// a comment naming /api/x is not counted\nfn page(v: Value) {\n  call("/api/c")\n}\n')
    r.write("src/core.rs", "pub fn decide(\n    a: u32,\n) -> u32 {\n    a\n}\n")
    return r


class Checks(unittest.TestCase):
    def test_holds_at_its_max(self):
        r = base_repo()
        code, out = r.run()
        self.assertEqual(code, 0, out)
        self.assertIn("| `routes` | R1 |  | 1 | 1 | holds |", out)

    def test_a_count_that_rises_fails_and_names_the_place(self):
        r = base_repo()
        r.write("src/core.rs", 'fn x() { call("/api/d") }\n')
        code, out = r.run()
        self.assertEqual(code, 1)
        self.assertIn("routes (R1): 2 places, 1 allowed", out)
        self.assertIn("src/core.rs:1:", out)

    def test_a_count_that_falls_asks_for_the_new_max(self):
        r = base_repo()
        r.write("src/web.rs", "fn page(v: Value) {}\n")
        code, out = r.run()
        self.assertEqual(code, 1)
        self.assertIn("Set max = 0", out)

    def test_a_max_above_0_needs_a_why(self):
        r = base_repo(why="")
        code, out = r.run()
        self.assertEqual(code, 1)
        self.assertIn("routes (R1): max is 1 with no why", out)
        self.assertIn("| `routes` | R1 |  | 1 | 1 | no why |", out)

    def test_a_blank_why_is_no_why(self):
        r = Repo()
        r.write(".tehas/repository.toml", pointer("v2"))
        r.write("architecture.toml", config().replace('why = "the page, until step 4"', 'why = "  "'))
        r.write("src/adapter.rs", 'fn get() { call("/api/a") }\n')
        r.write("src/web.rs", 'fn page(v: Value) { call("/api/c") }\n')
        code, out = r.run()
        self.assertEqual(code, 1)
        self.assertIn("routes (R1): max is 1 with no why", out)

    def test_a_max_of_0_needs_no_why(self):
        r = base_repo(routes=0, untyped=0, why="")
        r.write("src/web.rs", "fn page() {}\n")
        code, out = r.run()
        self.assertEqual(code, 0, out)

    def test_no_why_fails_with_base_too(self):
        r = base_repo(why="")
        r.commit()
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("routes (R1): max is 1 with no why", out)

    def test_allowed_files_and_comment_lines_are_not_counted(self):
        r = base_repo(routes=1)
        code, out = r.run()
        self.assertEqual(code, 0, out)

    def test_spans_cover_signatures_over_several_lines(self):
        r = base_repo()
        r.write("src/core.rs", "pub fn decide(\n    a: Value,\n) -> u32 {\n    let v: Value = 1;\n}\n")
        code, out = r.run()
        self.assertEqual(code, 1)
        self.assertIn("untyped (R4): 2 places, 1 allowed", out)
        self.assertIn("src/core.rs:1:", out)

    def test_the_body_after_the_end_is_not_part_of_the_span(self):
        r = base_repo()
        r.write("src/web.rs", 'fn page() { let v: Value = 1; call("/api/c") }\n')
        code, out = r.run()
        self.assertEqual(code, 1)
        self.assertIn("untyped (R4): 0 places, fewer than 1", out)

    def test_report_lists_without_failing(self):
        r = base_repo()
        code, out = r.run()
        self.assertEqual(code, 0, out)
        self.assertIn("**pub** (R5, reported, not checked)", out)
        self.assertIn("- `src/core.rs`: 1", out)

    def test_untracked_files_are_not_read(self):
        r = base_repo()
        with open(os.path.join(r.root, "src", "scratch.rs"), "w") as f:
            f.write('call("/api/z")\n')
        code, out = r.run()
        self.assertEqual(code, 0, out)


class Base(unittest.TestCase):
    def test_a_max_raised_without_a_new_why_fails(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config(routes=2))
        r.write("src/core.rs", 'fn x() { call("/api/d") }\n')
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("max raised from 1 to 2 without a new why", out)

    def test_a_max_raised_with_a_new_why_holds(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config(routes=2, why="the legacy page, until step 5"))
        r.write("src/core.rs", 'fn x() { call("/api/d") }\n')
        code, out = r.run("--base", "main")
        self.assertEqual(code, 0, out)
        self.assertIn("| `routes` | R1 | 1 | 2 | 2 | holds |", out)

    def test_a_wider_scope_without_a_new_why_fails(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config().replace('allowed = ["src/adapter.rs"]\n', ""))
        code, out = r.run("--base", "main", "--force")
        self.assertEqual(code, 1)
        self.assertIn("routes (R1): allowed changed from the base without a new why", out)
        self.assertIn("rescoped", out)

    def test_a_changed_pattern_and_files_without_a_new_why_fails(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config().replace("pattern = '/api/'", "pattern = '/api/c'")
                .replace('files = ["src/**/*.rs"]\nallowed', 'files = ["src/*.rs"]\nallowed'))
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("routes (R1): files, pattern changed from the base without a new why", out)

    def test_a_changed_start_end_and_comment_need_a_new_why(self):
        r = base_repo()
        r.commit()
        text = (config().replace("end = '[{;]'", "end = '[{]'").replace('comment = "//"', 'comment = "#"')
                .replace("start = '\\bfn\\s+\\w+'", "start = '\\bfn\\b'"))
        r.write("architecture.toml", text)
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("routes (R1): comment changed from the base without a new why", out)
        self.assertIn("untyped (R4): end, start changed from the base without a new why", out)

    def test_a_changed_mode_needs_a_new_why(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config().replace('mode = "spans"\n', 'mode = "lines"\n')
                .replace("start = '\\bfn\\s+\\w+'\n", "").replace("end = '[{;]'\n", ""))
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("untyped (R4): end, mode, start changed from the base without a new why", out)

    def test_a_narrowed_scope_at_max_0_without_a_why_fails(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config(routes=0, why="").replace("pattern = '/api/'", "pattern = '/api/zzz'"))
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("routes (R1): pattern changed from the base without a new why", out)
        self.assertIn("| `routes` | R1 | 1 | 0 | 0 | rescoped |", out)

    def test_a_removed_report_fails(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config().split("[[report]]")[0])
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("pub (R5): the report is in the base and gone here", out)

    def test_a_narrowed_report_needs_a_new_why(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config().replace("pattern = '^pub '", "pattern = '^pub fn '"))
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("pub (R5): pattern changed from the base without a new why", out)

    def test_a_lowered_top_needs_a_new_why(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config().replace('files = ["src/**"]\n', 'files = ["src/**"]\ntop = 3\n'))
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("pub (R5): top changed from the base without a new why", out)

    def test_a_rescoped_report_with_a_why_holds(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config().replace("pattern = '^pub '", "pattern = '^pub fn '") + 'why = "functions only, as the types are counted elsewhere"\n')
        code, out = r.run("--base", "main")
        self.assertEqual(code, 0, out)

    def test_a_report_top_written_at_its_default_is_not_a_change(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config() + "top = 10\n")
        code, out = r.run("--base", "main")
        self.assertEqual(code, 0, out)

    def test_a_key_written_at_its_default_is_not_a_change(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config().replace('id = "routes"\n', 'id = "routes"\nmode = "lines"\n'))
        code, out = r.run("--base", "main")
        self.assertEqual(code, 0, out)

    def test_a_changed_scope_with_a_new_why_holds(self):
        r = base_repo()
        r.commit()
        r.write("architecture.toml", config(routes=2, why="the adapter counted too, until step 4")
                .replace('allowed = ["src/adapter.rs"]\n', ""))
        code, out = r.run("--base", "main")
        self.assertEqual(code, 0, out)
        self.assertIn("| `routes` | R1 | 1 | 2 | 2 | holds |", out)

    def test_a_check_removed_from_the_base_fails(self):
        r = base_repo()
        r.commit()
        text = config()
        r.write("architecture.toml", text[: text.index("[[check]]\nid = \"untyped\"")] + text[text.index("[[report]]"):])
        code, out = r.run("--base", "main")
        self.assertEqual(code, 1)
        self.assertIn("untyped (R4): the check is in the base and gone here", out)

    def test_a_base_without_the_file_is_noted(self):
        r = Repo()
        r.write(".tehas/repository.toml", pointer("v2"))
        r.write("readme", "x\n")
        r.commit()
        r.write("architecture.toml", config(routes=0, untyped=0))
        r.write("src/a.rs", "fn a() {}\n")
        code, out = r.run("--base", "main")
        self.assertEqual(code, 0, out)
        self.assertIn("has no architecture.toml", out)


class Standard(unittest.TestCase):
    def breaking(self, standard):
        r = base_repo(standard=standard)
        r.write("src/core.rs", 'fn x() { call("/api/d") }\n')
        return r

    def test_v2_is_enforced(self):
        code, out = self.breaking("v2").run()
        self.assertEqual(code, 1, out)

    def test_v1_is_skipped(self):
        code, out = self.breaking("v1").run()
        self.assertEqual(code, 0, out)
        self.assertIn("Skipped: this repository's standard is v1", out)

    def test_no_file_is_skipped(self):
        code, out = self.breaking(None).run()
        self.assertEqual(code, 0, out)
        self.assertIn("is not set (no [standard] version in .tehas/repository.toml)", out)

    def test_force_runs_the_checks_at_v1(self):
        code, out = self.breaking("v1").run("--force")
        self.assertEqual(code, 1, out)
        self.assertIn("routes (R1): 2 places, 1 allowed", out)

    def test_comments_and_blank_lines_before_the_value(self):
        code, _ = self.breaking('# the architecture method\n\n[standard]\n# the version\nversion = "v2"').run()
        self.assertEqual(code, 1)

    def test_a_later_standard_needs_a_later_script(self):
        code, out = self.breaking("v3").run()
        self.assertEqual(code, 2)
        self.assertIn("standard v3 is newer than this script", out)

    def test_another_value_is_refused(self):
        code, out = self.breaking("strict").run()
        self.assertEqual(code, 2)
        self.assertIn("'strict' is not a standard", out)

    def test_a_table_without_a_version_is_refused(self):
        code, out = self.breaking("[standard]").run()
        self.assertEqual(code, 2)
        self.assertIn("[standard] has no version", out)

    def test_an_unknown_key_in_the_table_is_refused(self):
        code, out = self.breaking('[standard]\nversion = "v2"\nstrict = true').run()
        self.assertEqual(code, 2)
        self.assertIn("keys this script does not know: strict", out)

    def test_other_tables_are_left_to_their_owners(self):
        code, out = self.breaking('[standard]\nversion = "v2"\n\n[platform]\nos = "linux"').run()
        self.assertEqual(code, 1, out)

    def test_a_file_without_the_table_is_refused(self):
        for text in ['[platform]\nos = "linux"', '[Standard]\nversion = "v2"', '[standards]\nversion = "v2"', 'version = "v2"\n[x]', "# empty\n[x]"]:
            with self.subTest(text=text):
                code, out = self.breaking(text).run()
                self.assertEqual(code, 2, out)
                self.assertIn("no [standard] table", out)

    def test_standard_that_is_not_a_table_is_refused(self):
        for text in ['standard = "v2"\n[x]', '[[standard]]\nversion = "v2"']:
            with self.subTest(text=text):
                code, out = self.breaking(text).run()
                self.assertEqual(code, 2, out)
                self.assertIn("[standard] must be a table", out)

    def test_an_old_file_that_is_not_utf8_is_refused(self):
        r = self.breaking(None)
        os.makedirs(os.path.join(r.root, ".tehas"), exist_ok=True)
        with open(os.path.join(r.root, ".tehas/coding-standard"), "wb") as f:
            f.write(b"v\xff\n")
        code, out = r.run()
        self.assertEqual(code, 2, out)
        self.assertIn("not UTF-8", out)

    def test_a_symlink_is_refused_even_dangling(self):
        r = self.breaking(None)
        r.write(".tehas/coding-standard", "v1\n")
        os.symlink("nowhere.toml", os.path.join(r.root, ".tehas/repository.toml"))
        code, out = r.run()
        self.assertEqual(code, 2, out)
        self.assertIn("a symlink", out)

    def downgrade(self, before, after):
        r = base_repo(standard=None)
        if before:
            r.write(*before)
        r.commit()
        for name in (".tehas/repository.toml", ".tehas/coding-standard"):
            path = os.path.join(r.root, name)
            if os.path.lexists(path):
                os.remove(path)
        if after:
            r.write(*after)
        return r.run("--base", "main")

    def test_a_lower_standard_than_the_base_is_refused(self):
        code, out = self.downgrade((".tehas/repository.toml", pointer("v2")), (".tehas/repository.toml", pointer("v1")))
        self.assertEqual(code, 2, out)
        self.assertIn("the standard goes down from v2 at main to v1", out)

    def test_removing_the_file_the_base_has_is_refused(self):
        code, out = self.downgrade((".tehas/repository.toml", pointer("v2")), None)
        self.assertEqual(code, 2, out)
        self.assertIn("goes down from v2 at main to none", out)

    def test_the_move_from_the_old_file_is_not_a_downgrade(self):
        code, out = self.downgrade((".tehas/coding-standard", "v2\n"), (".tehas/repository.toml", pointer("v2")))
        self.assertNotIn("goes down", out)
        self.assertNotEqual(code, 2, out)

    def test_going_up_is_fine(self):
        code, out = self.downgrade((".tehas/repository.toml", pointer("v1")), (".tehas/repository.toml", pointer("v2")))
        self.assertNotIn("goes down", out)
        self.assertNotEqual(code, 2, out)

    def test_an_empty_file_is_refused(self):
        r = self.breaking(None)
        r.write(".tehas/repository.toml", "")
        code, out = r.run()
        self.assertEqual(code, 2, out)
        self.assertIn("no [standard] table", out)

    def test_a_version_that_is_not_a_string_is_refused(self):
        code, out = self.breaking("[standard]\nversion = 2").run()
        self.assertEqual(code, 2)
        self.assertIn("the version must be a string", out)

    def test_a_minor_needs_a_later_script(self):
        code, out = self.breaking("v2.1").run()
        self.assertEqual(code, 2)
        self.assertIn("standard v2.1 is newer than this script", out)

    def test_a_file_that_is_not_utf8_is_refused(self):
        r = self.breaking(None)
        os.makedirs(os.path.join(r.root, ".tehas"), exist_ok=True)
        with open(os.path.join(r.root, ".tehas/repository.toml"), "wb") as f:
            f.write(b"[standard]\nversion = \"\xff\"\n")
        code, out = r.run()
        self.assertEqual(code, 2, out)
        self.assertIn("not UTF-8", out)

    def test_bad_toml_is_refused(self):
        code, out = self.breaking("[standard\nversion = v2").run()
        self.assertEqual(code, 2)
        self.assertIn("not valid TOML", out)

    def test_the_old_name_is_read_for_v1(self):
        r = self.breaking(None)
        r.write(".tehas/coding-standard", "v1\n")
        code, out = r.run()
        self.assertEqual(code, 0, out)
        self.assertIn("Skipped: this repository's standard is v1", out)

    def test_the_old_name_with_v2_asks_for_the_move(self):
        r = self.breaking(None)
        r.write(".tehas/coding-standard", "v2\n")
        code, out = r.run()
        self.assertEqual(code, 2, out)
        self.assertIn('.tehas/coding-standard says v2; move it to .tehas/repository.toml as [standard] version = "v2"', out)

    def test_both_files_at_once_are_refused(self):
        r = self.breaking("v2")
        r.write(".tehas/coding-standard", "v1\n")
        code, out = r.run()
        self.assertEqual(code, 2, out)
        self.assertIn("are both here; keep .tehas/repository.toml alone", out)

    def test_an_empty_old_file_is_refused(self):
        r = self.breaking(None)
        r.write(".tehas/coding-standard", "# nothing\n")
        code, out = r.run()
        self.assertEqual(code, 2)
        self.assertIn("empty", out)


class Config(unittest.TestCase):
    def test_every_problem_at_once_and_exit_2(self):
        r = Repo()
        r.write(".tehas/repository.toml", pointer("v2"))
        r.write("architecture.toml", 'version = 1\nextra = 1\n[[check]]\nid = "a"\nrule = "R11"\nmode = "spans"\npattern = "("\n')
        code, out = r.run()
        self.assertEqual(code, 2)
        for want in ("unknown key 'extra'", "what is missing", "files is missing", "max is missing",
                     "rule must be one of R1 to R10", "mode spans needs start", "pattern:"):
            self.assertIn(want, out)

    def test_the_wrong_version_is_refused(self):
        r = Repo()
        r.write(".tehas/repository.toml", pointer("v2"))
        r.write("architecture.toml", "version = 2\n")
        code, out = r.run()
        self.assertEqual(code, 2)
        self.assertIn("version must be 1", out)

    def test_start_and_end_belong_to_spans(self):
        r = Repo()
        r.write(".tehas/repository.toml", pointer("v2"))
        r.write("architecture.toml", config().replace('mode = "spans"\n', ""))
        code, out = r.run()
        self.assertEqual(code, 2)
        self.assertIn("start and end are for mode spans", out)


class Globs(unittest.TestCase):
    def test_double_star_crosses_directories_and_star_does_not(self):
        g = architecture.glob_re("src/**/*.rs")
        self.assertTrue(g.match("src/a.rs"))
        self.assertTrue(g.match("src/web/api.rs"))
        self.assertFalse(g.match("src/a.rsx"))
        self.assertFalse(architecture.glob_re("src/*.rs").match("src/web/api.rs"))


if __name__ == "__main__":
    unittest.main()
