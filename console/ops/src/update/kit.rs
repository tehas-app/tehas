//! The checks of the shared kit and of the code that draws with its Solid, a
//! test of the workspace with the byte ratchet (host/docs/frontend.md, "The
//! kit" and "Solid 2 in tehas").
//!
//! - `host/fun/kit/MANIFEST` is `kit <n>`, then `file <sha256> <bytes> <name>`
//!   for every file of the kit but itself, by name, and nothing else.
//! - The library is the one file `fun/kit/vendor/solid.js`. Only a `.render.js`
//!   file and the kit files that draw (`DRAWS`) import it, by a relative
//!   address, and only by the names it exports.
//! - No file of `host/fun` names a Solid 1.x API (`OLD`).
//! - A file that imports the library builds no `html` template by `+`, by a
//!   call of `html(…)` or with an untagged template literal inside it, and
//!   writes no `style=` attribute into one: the pages run under a CSP with no
//!   inline style.

use super::pages::{imports, library, normal};
use sha2::{Digest, Sha256};
use std::fs;
use std::path::{Path, PathBuf};

/// The library, relative to `host`.
pub(super) const LIBRARY: &str = "fun/kit/vendor/solid.js";

/// The kit files that draw, and so may import the library.
pub(super) const DRAWS: [&str; 3] = ["fun/kit/feed.js", "fun/kit/parts.js", "fun/kit/state.js"];

/// Solid 1.x names that do not exist in 2.0, or are not to be used in tehas.
/// A word ending in `(` matches only as a call.
const OLD: [&str; 14] = [
    "createResource",
    "startTransition",
    "useTransition",
    "batch(",
    "onMount",
    "Suspense",
    "ErrorBoundary",
    "createMutable",
    "produce(",
    "mergeProps",
    "splitProps",
    "solid-js/web",
    "solid-js/store",
    "solid-js/html",
];

fn hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect()
}

/// Every file under `dir` of `root` but dot files, as paths relative to `root`,
/// in order.
fn files(root: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(root.join(dir)) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        if e.file_name().to_string_lossy().starts_with('.') {
            continue;
        }
        let path = dir.join(e.file_name());
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            files(root, &path, out);
        } else {
            out.push(path);
        }
    }
}

/// The manifest the kit at `kit` should have, with the contract number `n`.
fn manifest_of(kit: &Path, n: &str) -> String {
    let mut found = Vec::new();
    files(kit, Path::new(""), &mut found);
    let mut out = format!("kit {n}\n");
    for f in found {
        let name = f.to_string_lossy();
        if name == "MANIFEST" {
            continue;
        }
        let bytes = fs::read(kit.join(&f)).unwrap_or_default();
        out.push_str(&format!("file {} {} {name}\n", hex(&bytes), bytes.len()));
    }
    out
}

/// What is wrong with the manifest of the kit at `kit`, one line each.
pub(super) fn manifest(kit: &Path) -> Vec<String> {
    let Ok(text) = fs::read_to_string(kit.join("MANIFEST")) else {
        return vec!["fun/kit/MANIFEST: missing".to_string()];
    };
    let n = text
        .lines()
        .next()
        .and_then(|l| l.strip_prefix("kit "))
        .filter(|n| !n.is_empty() && n.bytes().all(|b| b.is_ascii_digit()));
    let Some(n) = n else {
        return vec!["fun/kit/MANIFEST: the first line is not \"kit <n>\"".to_string()];
    };
    let want = manifest_of(kit, n);
    if text == want {
        return Vec::new();
    }
    let mut wrong = Vec::new();
    for line in text.lines().skip(1) {
        if !want.lines().any(|w| w == line) {
            wrong.push(format!("fun/kit/MANIFEST: {line:?} is not the kit's file"));
        }
    }
    for line in want.lines().skip(1) {
        if !text.lines().any(|w| w == line) {
            wrong.push(format!("fun/kit/MANIFEST: no line {line:?}"));
        }
    }
    if wrong.is_empty() {
        wrong.push("fun/kit/MANIFEST: the lines are out of order".to_string());
    }
    wrong.push(format!(
        "fun/kit/MANIFEST should read (after a reviewed change to the kit):\n{want}"
    ));
    wrong
}

/// The names the library exports: its closing `export{a as B,…}`.
pub(super) fn exports(library: &str) -> Vec<String> {
    let Some(at) = library.rfind("export{") else {
        return Vec::new();
    };
    let list = &library[at + "export{".len()..];
    let list = &list[..list.find('}').unwrap_or(list.len())];
    list.split(',')
        .filter_map(|item| item.split(" as ").last())
        .map(|n| n.trim().to_string())
        .filter(|n| !n.is_empty())
        .collect()
}

/// The names of `import { a, b as c } from "<spec>"` statements of `text`, with
/// the address each comes from. A namespace or default import gives `*`.
pub(super) fn named_imports(text: &str) -> Vec<(String, String)> {
    let mut out = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find("import") {
        let start = at + offset;
        at = start + "import".len();
        let before = text[..start].chars().next_back();
        if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$') {
            continue;
        }
        let rest = &text[at..];
        let Some(from) = rest.find(" from ") else {
            continue;
        };
        let clause = rest[..from].trim();
        if clause.starts_with('(') || clause.contains(';') {
            continue;
        }
        let tail = rest[from + " from ".len()..].trim_start();
        let Some(quote) = tail.chars().next().filter(|c| *c == '"' || *c == '\'') else {
            continue;
        };
        let Some(end) = tail[1..].find(quote) else {
            continue;
        };
        let spec = tail[1..1 + end].to_string();
        match (clause.find('{'), clause.find('}')) {
            (Some(open), Some(close)) if open < close => {
                for item in clause[open + 1..close].split(',') {
                    if let Some(name) = item.split_whitespace().next() {
                        out.push((name.to_string(), spec.clone()));
                    }
                }
                if clause[..open].trim().trim_end_matches(',').trim() != "" {
                    out.push(("*".to_string(), spec.clone()));
                }
            }
            _ => out.push(("*".to_string(), spec.clone())),
        }
    }
    out
}

/// Whether `word` stands in `text` as a name of its own: not inside a longer
/// name, and for a call not as a method.
fn names(text: &str, word: &str) -> bool {
    let ident = |c: char| c.is_alphanumeric() || c == '_' || c == '$';
    let call = word.ends_with('(');
    text.match_indices(word).any(|(at, _)| {
        let before = text[..at].chars().next_back();
        let after = text[at + word.len()..].chars().next();
        !before.is_some_and(|c| ident(c) || (call && c == '.'))
            && (call || word.contains('/') || !after.is_some_and(ident))
    })
}

/// The Solid 1.x names `text` holds.
pub(super) fn old_names(text: &str) -> Vec<&'static str> {
    OLD.into_iter().filter(|w| names(text, w)).collect()
}

/// What is wrong with the `html` templates of `text`, one line each.
pub(super) fn templates(text: &str) -> Vec<String> {
    let b = text.as_bytes();
    let ident = |c: u8| c.is_ascii_alphanumeric() || c == b'_' || c == b'$' || c == b'.';
    let mut wrong = Vec::new();
    for (at, _) in text.match_indices("html") {
        if at > 0 && ident(b[at - 1]) {
            continue;
        }
        let next = at + 4;
        match b.get(next) {
            Some(b'(') => wrong.push("calls html(…): write html`…` with constant text".to_string()),
            Some(b'`') => {
                let before = text[..at].trim_end();
                if before.ends_with('+') {
                    wrong.push("joins a string to an html template with +".to_string());
                }
                match template(text, next) {
                    Ok(end) => {
                        if text[end..].trim_start().starts_with('+') {
                            wrong.push("joins an html template to a string with +".to_string());
                        }
                    }
                    Err(why) => wrong.push(why),
                }
            }
            _ => {}
        }
    }
    wrong.sort();
    wrong.dedup();
    wrong
}

/// Scans the template literal whose backtick is at `open` to the byte past its
/// closing one. Inside an `html` template a `${…}` may hold a tagged `html`
/// template, but no untagged one, and the constant text has no `style=`.
fn template(text: &str, open: usize) -> Result<usize, String> {
    let b = text.as_bytes();
    let mut i = open + 1;
    let mut constant = String::new();
    while i < b.len() {
        match b[i] {
            b'\\' => i += 2,
            b'`' => {
                if constant.contains("style=") {
                    return Err("writes a style= attribute in an html template; use a class".into());
                }
                return Ok(i + 1);
            }
            b'$' if b.get(i + 1) == Some(&b'{') => {
                i = expression(text, i + 2)?;
            }
            c => {
                constant.push(c as char);
                i += 1;
            }
        }
    }
    Err("an html template that does not end".into())
}

/// Scans a `${…}` from the byte after `{` to the byte past its `}`.
fn expression(text: &str, from: usize) -> Result<usize, String> {
    let b = text.as_bytes();
    let mut depth = 0usize;
    let mut i = from;
    while i < b.len() {
        match b[i] {
            b'{' => depth += 1,
            b'}' if depth == 0 => return Ok(i + 1),
            b'}' => depth -= 1,
            b'"' | b'\'' => {
                let quote = b[i];
                i += 1;
                while i < b.len() && b[i] != quote {
                    i += if b[i] == b'\\' { 2 } else { 1 };
                }
            }
            b'`' => {
                if !text[..i].ends_with("html") {
                    return Err(
                        "puts an untagged template literal in an html template; bind the value as an expression"
                            .into(),
                    );
                }
                i = template(text, i)? - 1;
            }
            _ => {}
        }
        i += 1;
    }
    Err("an html template expression that does not end".into())
}

/// The lints of the files of `fun` under `host`, given as paths relative to
/// `host`, one line each.
pub(super) fn lint(host: &Path, all: &[PathBuf]) -> Vec<String> {
    let mut wrong = Vec::new();
    let exported = exports(&fs::read_to_string(host.join(LIBRARY)).unwrap_or_default());
    for f in all {
        let name = f.to_string_lossy();
        if name.starts_with("fun/kit/vendor/") {
            continue;
        }
        let Ok(text) = fs::read_to_string(host.join(f)) else {
            wrong.push(format!("{name}: cannot read"));
            continue;
        };
        for old in old_names(&text) {
            wrong.push(format!("{name}: names {old:?}, a Solid 1.x API"));
        }
        // Any other file that imports a library is refused by pages.rs.
        if !(name.ends_with(".render.js") || DRAWS.contains(&name.as_ref())) {
            continue;
        }
        let dir = f.parent().unwrap_or(Path::new(""));
        let is_library =
            |spec: &str| normal(&dir.join(spec)).as_deref() == Some(Path::new(LIBRARY));
        for spec in imports(&text) {
            if library(&spec) && !is_library(&spec) {
                wrong.push(format!(
                    "{name}: imports {spec:?}; the one library is {LIBRARY}"
                ));
            }
        }
        let mut uses = false;
        for (import, spec) in named_imports(&text) {
            if !is_library(&spec) {
                continue;
            }
            uses = true;
            if import == "*" {
                wrong.push(format!(
                    "{name}: imports the library whole; name what it uses"
                ));
            } else if !exported.contains(&import) {
                wrong.push(format!(
                    "{name}: imports {import:?}, which the kit's Solid does not export"
                ));
            }
        }
        if uses {
            for why in templates(&text) {
                wrong.push(format!("{name}: {why}"));
            }
        }
    }
    wrong
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;

    #[test]
    fn the_manifest_names_every_file_with_its_sha256_and_size() {
        let d = dir("kit-manifest");
        let kit = d.0.join("kit");
        d.file("kit/a.js", "a");
        d.file("kit/vendor/solid.js", "bb");
        d.file("kit/.hidden", "x");
        let good = format!(
            "kit 1\nfile {} 1 a.js\nfile {} 2 vendor/solid.js\n",
            hex(b"a"),
            hex(b"bb")
        );
        d.file("kit/MANIFEST", &good);
        assert_eq!(manifest(&kit), Vec::<String>::new());

        d.file("kit/a.js", "A");
        let wrong = manifest(&kit);
        assert!(
            wrong[0].contains("1 a.js\" is not the kit's file"),
            "{wrong:?}"
        );
        assert!(
            wrong[1].starts_with("fun/kit/MANIFEST: no line"),
            "{wrong:?}"
        );
        assert!(wrong.last().unwrap().contains(&hex(b"A")));

        d.file("kit/a.js", "a");
        d.file("kit/new.js", "n");
        assert!(manifest(&kit)[0].contains("no line \"file"));
        fs::remove_file(kit.join("new.js")).unwrap();

        d.file("kit/MANIFEST", &good.replace("kit 1", "kit one"));
        assert_eq!(
            manifest(&kit),
            ["fun/kit/MANIFEST: the first line is not \"kit <n>\""]
        );
        fs::remove_file(kit.join("MANIFEST")).unwrap();
        assert_eq!(manifest(&kit), ["fun/kit/MANIFEST: missing"]);
    }

    #[test]
    fn the_exports_are_the_closing_export_list() {
        assert_eq!(
            exports("var a=1;export{Fd as Errored,jd as For,ap as html,B as untrack};"),
            ["Errored", "For", "html", "untrack"]
        );
        assert!(exports("var a;").is_empty());
    }

    #[test]
    fn named_imports_are_read_with_their_address() {
        let text = "import { For, html as h,\n  Show } from \"./vendor/solid.js\";\n\
                    import * as S from '../kit/vendor/solid.js';\n\
                    import x, { y } from \"./z.js\";\nconst m = import(\"./lazy.js\");";
        let got = named_imports(text);
        let names: Vec<&str> = got.iter().map(|(n, _)| n.as_str()).collect();
        assert_eq!(names, ["For", "html", "Show", "*", "y", "*"]);
        assert_eq!(got[3].1, "../kit/vendor/solid.js");
    }

    #[test]
    fn a_solid_1_name_is_found_as_a_name_and_not_inside_another() {
        assert_eq!(
            old_names("const [d] = createResource(f); onMount(() => {});"),
            ["createResource", "onMount"]
        );
        assert_eq!(
            old_names("import { render } from \"solid-js/web\";"),
            ["solid-js/web"]
        );
        assert_eq!(
            old_names("batch(() => x()); produce(d)"),
            ["batch(", "produce("]
        );
        assert!(old_names("queue.batch(1); unbatch(); onMountain; mySuspense").is_empty());
        assert_eq!(old_names("<Suspense fallback>"), ["Suspense"]);
    }

    #[test]
    fn an_html_template_is_constant_text_with_bound_expressions() {
        let good = "html`<p class=\"a\">${() => m.title}</p>\n${html`<b>${x}</b>`}<${For} each=${() => [1]}>${(r) => html`<i>${r}</i>`}<//>`";
        assert_eq!(templates(good), Vec::<String>::new());
        assert_eq!(
            templates("html`<p>` + title"),
            ["joins an html template to a string with +"]
        );
        assert_eq!(
            templates("'<b>' + html`<p></p>`"),
            ["joins a string to an html template with +"]
        );
        assert_eq!(
            templates("html`<p>${`<b>${x}</b>`}</p>`"),
            ["puts an untagged template literal in an html template; bind the value as an expression"]
        );
        assert_eq!(
            templates("html([\"<p>\" + t + \"</p>\"])"),
            ["calls html(…): write html`…` with constant text"]
        );
        assert_eq!(
            templates("html`<p style=\"color: red\">${x}</p>`"),
            ["writes a style= attribute in an html template; use a class"]
        );
        assert!(templates("html`<p>${\"}\"}</p>`").is_empty());
        assert!(templates("const html = 1; xhtml`a` + 1").is_empty());
    }

    /// A host whose library exports `For` and `html`.
    fn host(name: &str) -> super::super::testing::Dir {
        let d = dir(name);
        d.file(
            "host/fun/kit/vendor/solid.js",
            "var a;export{a as For,b as html};",
        );
        d
    }

    fn run(d: &super::super::testing::Dir) -> Vec<String> {
        let host = d.0.join("host");
        let mut all = Vec::new();
        files(&host, Path::new("fun"), &mut all);
        lint(&host, &all)
    }

    #[test]
    fn a_file_that_draws_imports_the_one_library_and_no_other() {
        let d = host("kit-library");
        d.file(
            "host/fun/q.render.js",
            "import { For, html } from \"./kit/vendor/solid.js\";\nimport { h } from \"./kit/vendor/preact.js\";",
        );
        d.file(
            "host/fun/kit/feed.js",
            "import { For } from \"./vendor/solid.js\";",
        );
        d.file(
            "host/fun/kit/parts.js",
            "import { html } from \"solid-js\";",
        );
        // pages.rs refuses this one: it is no file that draws.
        d.file(
            "host/fun/q.js",
            "import { For } from \"./kit/vendor/solid.js\";",
        );
        assert_eq!(
            run(&d),
            [
                "fun/kit/parts.js: imports \"solid-js\"; the one library is fun/kit/vendor/solid.js",
                "fun/q.render.js: imports \"./kit/vendor/preact.js\"; the one library is fun/kit/vendor/solid.js",
            ]
        );
    }

    #[test]
    fn a_name_the_library_does_not_export_or_a_whole_import_is_refused() {
        let d = host("kit-names");
        d.file(
            "host/fun/q.render.js",
            "import { For, Suspense2 } from \"./kit/vendor/solid.js\";\nimport * as S from \"./kit/vendor/solid.js\";",
        );
        assert_eq!(
            run(&d),
            [
                "fun/q.render.js: imports \"Suspense2\", which the kit's Solid does not export",
                "fun/q.render.js: imports the library whole; name what it uses",
            ]
        );
    }

    #[test]
    fn an_old_name_or_a_built_template_in_a_file_of_fun_is_refused() {
        let d = host("kit-old");
        d.file("host/fun/legacy.js", "// onMount runs once\n");
        d.file(
            "host/fun/q.render.js",
            "import { html } from \"./kit/vendor/solid.js\";\nexport const v = (t) => html`<p>` + t;",
        );
        d.file("host/fun/plain.js", "const s = `<p>${`x`}</p>` + 'html`';");
        assert_eq!(
            run(&d),
            [
                "fun/legacy.js: names \"onMount\", a Solid 1.x API",
                "fun/q.render.js: joins an html template to a string with +",
            ]
        );
    }
}
