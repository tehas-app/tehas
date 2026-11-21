//! The byte ratchet of the dashboard's pages and islands, a test of the
//! workspace (host/docs/frontend.md, "Budgets").
//!
//! `host/fun/pages.max` holds a ceiling for each page and each island:
//!
//!   page <name> <script bytes> <markup and style bytes>
//!   island <name> <bytes>
//!
//! A page's script bytes are the files its `<script src="/fun/…">` tags name
//! and the modules those import by a relative address, each file once. Its
//! markup and style bytes are the page's own files and the stylesheets its
//! `<link href="/fun/…">` tags name. The theme file is left out: a template
//! names it. An island is the three files of a kind, `<name>.js`,
//! `<name>.model.js` and `<name>.render.js`, found by its `.render.js` file
//! under `host/fun`; its line may not pass the island budget.
//!
//! The library, `fun/kit/vendor/solid.js`, is counted once, on its own line,
//! and in no page's script bytes:
//!
//!   library solid <bytes>
//!
//! A page or island over its line fails, and so does one with no line or a
//! line with nothing to measure. A change that grows a page raises its line in
//! the same change; a change that shrinks one may lower it. A library import,
//! a bare address or one through a `vendor/` directory, is allowed only in a
//! `.render.js` file and the kit files that draw, and an element or model file
//! may not reach one of those by its relative imports, other than through a
//! `.render.js` file. The kit's manifest and the
//! lints of the code that draws with Solid are in `kit.rs`.

use super::assets::{address, in_comment, served, spans, PAGES as STAGED};
use super::kit::{self, DRAWS, LIBRARY, TYPES};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The pages, each with the files of `host` that make it. The dashboard is a
/// fragment the manual's layout renders.
const PAGES: [(&str, &[&str]); 5] = [
    ("business", &["fun/business.html"]),
    ("floor", &["fun/index.html"]),
    ("dashboard", &["docs/_layout.html", "docs/dashboard.frag"]),
    ("wiki", &["docs/_layout.html", "docs/wiki.frag"]),
    ("manual", &["docs/_layout.html"]),
];

/// The most an island's three files may hold together.
const ISLAND_BUDGET: u64 = 12 * 1024;

#[derive(Debug, PartialEq)]
struct Size {
    script: u64,
    markup: u64,
}

fn bytes(path: &Path) -> Result<u64, String> {
    fs::metadata(path)
        .map(|m| m.len())
        .map_err(|e| format!("{}: {e}", path.display()))
}

/// `path` with `.` and `..` taken out by name. Nothing when it climbs out of
/// its root.
pub(super) fn normal(path: &Path) -> Option<PathBuf> {
    let mut out = PathBuf::new();
    for c in path.components() {
        match c {
            Component::CurDir => {}
            Component::ParentDir => {
                if !out.pop() {
                    return None;
                }
            }
            other => out.push(other),
        }
    }
    Some(out)
}

/// The addresses a module imports: `import … from "x"`, `export … from "x"`,
/// `import "x"` and `import("x")`, in the order they stand. An address of a
/// `.d.ts` file in a `/* */` comment is a type JSDoc names, which no browser
/// loads, so it is left out.
pub(super) fn imports(text: &str) -> Vec<String> {
    let mut found = Vec::new();
    for word in ["import", "from"] {
        let mut at = 0;
        while let Some(offset) = text[at..].find(word) {
            let start = at + offset;
            at = start + word.len();
            let before = text[..start].chars().next_back();
            if before.is_some_and(|c| c.is_alphanumeric() || c == '_' || c == '.' || c == '$') {
                continue;
            }
            let rest = text[at..].trim_start();
            let rest = rest.strip_prefix('(').map_or(rest, str::trim_start);
            let Some(quote) = rest.chars().next().filter(|c| *c == '"' || *c == '\'') else {
                continue;
            };
            let value = &rest[1..];
            if let Some(end) = value.find(quote) {
                if !value[..end].contains('\n')
                    && (served(&value[..end]) || !in_comment(text, start))
                {
                    found.push((start, value[..end].to_string()));
                }
            }
        }
    }
    found.sort();
    found.into_iter().map(|(_, s)| s).collect()
}

fn relative(spec: &str) -> bool {
    spec.starts_with("./") || spec.starts_with("../")
}

/// Whether an import address names a library: a bare name, or a file in a
/// `vendor/` directory.
pub(super) fn library(spec: &str) -> bool {
    !(relative(spec) || spec.starts_with('/')) || spec.split('/').any(|part| part == "vendor")
}

/// A script and the modules it imports by a relative address, at any depth,
/// into `seen`. A module that is not there counts for nothing.
fn follow(host: &Path, file: PathBuf, seen: &mut BTreeSet<PathBuf>) -> Result<(), String> {
    if !seen.insert(file.clone()) {
        return Ok(());
    }
    let text =
        fs::read_to_string(host.join(&file)).map_err(|e| format!("{}: {e}", file.display()))?;
    let dir = file.parent().unwrap_or(Path::new(""));
    for spec in imports(&text).into_iter().filter(|s| relative(s)) {
        if let Some(next) = normal(&dir.join(&spec)) {
            if host.join(&next).is_file() {
                follow(host, next, seen)?;
            }
        }
    }
    Ok(())
}

/// The library or kit file that draws which `file` reaches by its relative
/// imports, not counting the way through a `.render.js` file. Nothing when it
/// reaches neither.
fn draws_through(host: &Path, file: &Path) -> Option<PathBuf> {
    let mut seen = BTreeSet::new();
    let mut todo = vec![file.to_path_buf()];
    while let Some(at) = todo.pop() {
        if !seen.insert(at.clone()) {
            continue;
        }
        let Ok(text) = fs::read_to_string(host.join(&at)) else {
            continue;
        };
        let dir = at.parent().unwrap_or(Path::new(""));
        for spec in imports(&text).into_iter().filter(|s| relative(s)) {
            let Some(next) = normal(&dir.join(&spec)) else {
                continue;
            };
            let name = next.to_string_lossy();
            if name.ends_with(".render.js") {
                continue;
            }
            if name == LIBRARY || DRAWS.contains(&name.as_ref()) {
                return Some(next);
            }
            todo.push(next);
        }
    }
    None
}

/// The size of the page that `files` of `host` make.
fn measure(host: &Path, files: &[&str]) -> Result<Size, String> {
    let mut scripts = BTreeSet::new();
    let mut styles = BTreeSet::new();
    let mut markup = 0;
    for name in files {
        let text = fs::read_to_string(host.join(name)).map_err(|e| format!("{name}: {e}"))?;
        markup += text.len() as u64;
        for (open, close) in spans(&text) {
            let Some((_, addr)) = address(&text[open..close]) else {
                continue;
            };
            if addr.contains("{{") {
                continue;
            }
            let Some(file) = normal(Path::new(addr.trim_start_matches('/'))) else {
                continue;
            };
            if !host.join(&file).is_file() {
                continue;
            }
            if addr.ends_with(".js") {
                follow(host, file, &mut scripts)?;
            } else if addr.ends_with(".css") {
                styles.insert(file);
            }
        }
    }
    scripts.remove(Path::new(LIBRARY));
    let mut script = 0;
    for f in &scripts {
        script += bytes(&host.join(f))?;
    }
    for f in &styles {
        markup += bytes(&host.join(f))?;
    }
    Ok(Size { script, markup })
}

/// Every `.js` file under `dir`, as paths relative to `host`, in order.
fn modules(host: &Path, dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = fs::read_dir(host.join(dir)) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(Result::ok).collect();
    entries.sort_by_key(|e| e.file_name());
    for e in entries {
        let path = dir.join(e.file_name());
        if e.file_type().is_ok_and(|t| t.is_dir()) {
            modules(host, &path, out);
        } else if path.extension().is_some_and(|x| x == "js") {
            out.push(path);
        }
    }
}

/// The islands under `host/fun`, by name (the path under `fun` before
/// `.render.js`), each with the bytes of its three files.
fn islands(host: &Path, files: &[PathBuf]) -> Result<Vec<(String, u64)>, String> {
    let mut out = Vec::new();
    for f in files {
        let text = f.to_string_lossy();
        let Some(stem) = text.strip_suffix(".render.js") else {
            continue;
        };
        let mut total = 0;
        for part in [".js", ".model.js", ".render.js"] {
            let path = host.join(format!("{stem}{part}"));
            if path.is_file() {
                total += bytes(&path)?;
            }
        }
        out.push((stem.trim_start_matches("fun/").to_string(), total));
    }
    Ok(out)
}

/// What is wrong with `host` against the ceilings in `max`, one line each.
/// Nothing when every page and island is at or under its line.
fn check(host: &Path, max: &str) -> Vec<String> {
    let mut wrong = Vec::new();
    let mut pages: Vec<(String, Size)> = Vec::new();
    let mut island_lines: Vec<(String, u64)> = Vec::new();
    let mut library_line: Option<u64> = None;
    for (n, line) in max.lines().enumerate() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let words: Vec<&str> = line.split_whitespace().collect();
        let num = |w: &str| w.parse::<u64>().ok();
        match words.as_slice() {
            ["page", name, s, m] if num(s).is_some() && num(m).is_some() => pages.push((
                name.to_string(),
                Size {
                    script: num(s).unwrap_or(0),
                    markup: num(m).unwrap_or(0),
                },
            )),
            ["island", name, b] if num(b).is_some() => {
                island_lines.push((name.to_string(), num(b).unwrap_or(0)))
            }
            ["library", "solid", b] if num(b).is_some() && library_line.is_none() => {
                library_line = num(b)
            }
            _ => wrong.push(format!("pages.max line {}: cannot read {line:?}", n + 1)),
        }
    }

    for (name, files) in PAGES {
        let Some((_, ceiling)) = pages.iter().find(|(n, _)| n == name) else {
            wrong.push(format!("page {name}: no line in pages.max"));
            continue;
        };
        match measure(host, files) {
            Err(e) => wrong.push(format!("page {name}: {e}")),
            Ok(size) => {
                if size.script > ceiling.script {
                    wrong.push(format!(
                        "page {name}: script {} bytes, the ceiling is {}",
                        size.script, ceiling.script
                    ));
                }
                if size.markup > ceiling.markup {
                    wrong.push(format!(
                        "page {name}: markup and style {} bytes, the ceiling is {}",
                        size.markup, ceiling.markup
                    ));
                }
            }
        }
    }
    for (name, _) in &pages {
        if !PAGES.iter().any(|(p, _)| p == name) {
            wrong.push(format!("page {name}: in pages.max but no such page"));
        }
    }

    match (host.join(LIBRARY).is_file(), library_line) {
        (false, None) => {}
        (false, Some(_)) => wrong.push(format!("library solid: in pages.max but no {LIBRARY}")),
        (true, None) => wrong.push("library solid: no line in pages.max".to_string()),
        (true, Some(ceiling)) => match bytes(&host.join(LIBRARY)) {
            Err(e) => wrong.push(format!("library solid: {e}")),
            Ok(size) if size > ceiling => wrong.push(format!(
                "library solid: {size} bytes, the ceiling is {ceiling}"
            )),
            Ok(_) => {}
        },
    }
    if host.join("fun/kit").is_dir() {
        wrong.extend(kit::manifest(&host.join("fun/kit")));
    }
    if let Ok(library) = fs::read_to_string(host.join(LIBRARY)) {
        match fs::read_to_string(host.join(TYPES)) {
            Ok(dts) => wrong.extend(kit::types(&library, &dts)),
            Err(e) => wrong.push(format!("{TYPES}: {e}; the library has its types")),
        }
    }

    let mut files = Vec::new();
    modules(host, Path::new("fun"), &mut files);
    match islands(host, &files) {
        Err(e) => wrong.push(e),
        Ok(found) => {
            for (name, size) in &found {
                match island_lines.iter().find(|(n, _)| n == name) {
                    None => wrong.push(format!("island {name}: no line in pages.max")),
                    Some((_, ceiling)) if size > ceiling => wrong.push(format!(
                        "island {name}: {size} bytes, the ceiling is {ceiling}"
                    )),
                    Some(_) => {}
                }
            }
            for (name, ceiling) in &island_lines {
                if !found.iter().any(|(n, _)| n == name) {
                    wrong.push(format!("island {name}: in pages.max but no such island"));
                }
                if *ceiling > ISLAND_BUDGET {
                    wrong.push(format!(
                        "island {name}: the ceiling {ceiling} is over the budget of {ISLAND_BUDGET}"
                    ));
                }
            }
        }
    }

    wrong.extend(kit::lint(host, &files));
    for f in &files {
        let Ok(text) = fs::read_to_string(host.join(f)) else {
            continue;
        };
        for spec in imports(&text).into_iter().filter(|s| !served(s)) {
            wrong.push(format!(
                "{}: imports {spec:?}, a type file no home has; name it in a JSDoc comment",
                f.display()
            ));
        }
    }
    for f in &files {
        let name = f.to_string_lossy();
        if name.ends_with(".render.js") || DRAWS.contains(&name.as_ref()) {
            continue;
        }
        let Ok(text) = fs::read_to_string(host.join(f)) else {
            wrong.push(format!("{}: cannot read", f.display()));
            continue;
        };
        for spec in imports(&text).into_iter().filter(|s| library(s)) {
            wrong.push(format!(
                "{}: imports the library {spec:?}; only a .render.js file or a kit file that draws may",
                f.display()
            ));
        }
        // An element or a model loads nothing that draws: an element would
        // load the library before its hydrate trigger fires.
        let element = name
            .strip_suffix(".js")
            .is_some_and(|stem| host.join(format!("{stem}.render.js")).is_file());
        if element || name.ends_with(".model.js") {
            if let Some(draws) = draws_through(host, f) {
                wrong.push(format!(
                    "{}: reaches {} by its imports; an element or model file loads nothing that draws",
                    f.display(),
                    draws.display()
                ));
            }
        }
    }
    wrong
}

#[cfg(test)]
mod tests {
    use super::super::testing::{dir, Dir};
    use super::*;
    use sha2::Digest;

    fn host_of(d: &Dir) -> PathBuf {
        d.0.join("host")
    }

    /// A host with the four pages, one module that imports another, and a
    /// theme named by a template.
    fn small(name: &str) -> Dir {
        let d = dir(name);
        d.file(
            "host/fun/business.html",
            "<link rel=\"stylesheet\" href=\"/fun/theme-{{ $design }}.css\">\n\
             <link rel=\"stylesheet\" href=\"/fun/a.css\">\n\
             <script src=\"/fun/one.js\"></script>\n\
             <script type=\"module\" src=\"/fun/view.js\"></script>\n\
             <script async src=\"{{ env \"X\" }}\"></script>",
        );
        d.file("host/fun/index.html", "<p>floor</p>");
        d.file("host/docs/_layout.html", "<main></main>");
        d.file(
            "host/docs/dashboard.frag",
            "<script src=\"/fun/one.js\"></script>",
        );
        d.file("host/docs/wiki.frag", "<p>wiki</p>");
        d.file("host/fun/a.css", "a{}");
        d.file("host/fun/theme-pixel.css", "/* a theme */");
        d.file("host/fun/one.js", "1;");
        d.file(
            "host/fun/view.js",
            "import { x } from \"./data.js\";\nimport \"./one.js\";",
        );
        d.file("host/fun/data.js", "export const x = 1;");
        d
    }

    fn size(d: &Dir, files: &[&str]) -> Size {
        measure(&host_of(d), files).unwrap()
    }

    fn len(d: &Dir, name: &str) -> u64 {
        fs::metadata(host_of(d).join(name)).unwrap().len()
    }

    /// pages.max at exactly today's sizes of `d`.
    fn exact(d: &Dir) -> String {
        let mut out = String::new();
        for (name, files) in PAGES {
            let s = size(d, files);
            out.push_str(&format!("page {name} {} {}\n", s.script, s.markup));
        }
        out
    }

    #[test]
    fn a_page_counts_its_scripts_their_imports_once_and_its_stylesheets() {
        let d = small("pages-measure");
        let s = size(&d, &["fun/business.html"]);
        let script: u64 = ["fun/one.js", "fun/view.js", "fun/data.js"]
            .iter()
            .map(|f| len(&d, f))
            .sum();
        assert_eq!(s.script, script);
        // The page and a.css; the theme a template names is left out.
        assert_eq!(
            s.markup,
            len(&d, "fun/business.html") + len(&d, "fun/a.css")
        );
    }

    #[test]
    fn the_dashboard_counts_the_layout_and_its_fragment() {
        let d = small("pages-dashboard");
        let s = size(&d, &["docs/_layout.html", "docs/dashboard.frag"]);
        assert_eq!(s.script, len(&d, "fun/one.js"));
        assert_eq!(
            s.markup,
            len(&d, "docs/_layout.html") + len(&d, "docs/dashboard.frag")
        );
    }

    #[test]
    fn the_imports_of_a_module_are_its_static_reexported_and_dynamic_ones() {
        let text = "import a from \"./a.js\";\nimport {\n  b,\n  c,\n} from './b.js';\n\
                    export { d } from \"../d.js\";\nimport \"./e.js\";\nconst f = import(\"./f.js\");\n\
                    reimport(\"./no.js\"); obj.import(\"./no2.js\");";
        assert_eq!(
            imports(text),
            ["./a.js", "./b.js", "../d.js", "./e.js", "./f.js"]
        );
    }

    #[test]
    fn a_page_at_its_ceiling_passes_and_one_byte_over_fails() {
        let d = small("pages-ceiling");
        let host = host_of(&d);
        let max = exact(&d);
        assert_eq!(check(&host, &max), Vec::<String>::new());
        d.file("host/fun/data.js", "export const x = 10;");
        let wrong = check(&host, &max);
        assert_eq!(wrong.len(), 1, "{wrong:?}");
        assert!(wrong[0].starts_with("page business: script"), "{wrong:?}");
        d.file("host/fun/a.css", "a{ }");
        assert!(check(&host, &max)
            .iter()
            .any(|w| w.starts_with("page business: markup and style")));
    }

    #[test]
    fn a_page_with_no_line_or_a_line_with_no_page_fails() {
        let d = small("pages-lines");
        let host = host_of(&d);
        let max = exact(&d).replace("page floor", "page flour");
        let wrong = check(&host, &max);
        assert!(
            wrong.contains(&"page floor: no line in pages.max".to_string()),
            "{wrong:?}"
        );
        assert!(wrong.contains(&"page flour: in pages.max but no such page".to_string()));
        let wrong = check(&host, &format!("{}page floor 1\n", exact(&d)));
        assert!(
            wrong[0].starts_with("pages.max line 6: cannot read"),
            "{wrong:?}"
        );
    }

    #[test]
    fn an_island_needs_a_line_under_the_budget_and_stays_under_it() {
        let d = small("pages-island");
        let host = host_of(&d);
        d.file("host/fun/board/log.js", "aaaa");
        d.file("host/fun/board/log.model.js", "bb");
        d.file("host/fun/board/log.render.js", "c");
        let wrong = check(&host, &exact(&d));
        assert_eq!(wrong, ["island board/log: no line in pages.max"]);
        assert!(check(&host, &format!("{}island board/log 7\n", exact(&d))).is_empty());
        let wrong = check(&host, &format!("{}island board/log 6\n", exact(&d)));
        assert_eq!(wrong, ["island board/log: 7 bytes, the ceiling is 6"]);
        let wrong = check(&host, &format!("{}island board/log 12289\n", exact(&d)));
        assert_eq!(
            wrong,
            ["island board/log: the ceiling 12289 is over the budget of 12288"]
        );
        let wrong = check(
            &host,
            &format!("{}island board/log 7\nisland gone 1\n", exact(&d)),
        );
        assert_eq!(wrong, ["island gone: in pages.max but no such island"]);
    }

    #[test]
    fn only_a_render_file_may_import_a_library() {
        let d = small("pages-library");
        let host = host_of(&d);
        d.file(
            "host/fun/board/log.js",
            "import { h } from \"./vendor/preact.js\";",
        );
        d.file("host/fun/board/log.model.js", "import x from \"lib\";");
        d.file(
            "host/fun/board/log.render.js",
            "import { h } from \"./vendor/preact.js\";",
        );
        let max = format!("{}island board/log 1000\n", exact(&d));
        assert_eq!(
            check(&host, &max),
            [
                "fun/board/log.render.js: imports \"./vendor/preact.js\"; the one library is fun/kit/vendor/solid.js",
                "fun/board/log.js: imports the library \"./vendor/preact.js\"; only a .render.js file or a kit file that draws may",
                "fun/board/log.model.js: imports the library \"lib\"; only a .render.js file or a kit file that draws may",
            ]
        );
    }

    #[test]
    fn an_element_or_model_file_reaches_nothing_that_draws() {
        let d = with_kit("pages-reach");
        let host = host_of(&d);
        d.file(
            "host/fun/view.js",
            "import { x } from \"./data.js\";\nimport \"./one.js\";",
        );
        d.file(
            "host/fun/board/log.js",
            "import { island } from \"../kit/island.js\";\nimport { useBoard } from \"../kit/state.js\";\n\
             island(\"log\", { load: () => import(\"./log.render.js\") });",
        );
        d.file(
            "host/fun/board/log.model.js",
            "import { f } from \"./help.js\";",
        );
        d.file(
            "host/fun/board/help.js",
            "export { feed } from \"../kit/parts.js\";",
        );
        d.file(
            "host/fun/board/log.render.js",
            "import { html } from \"../kit/vendor/solid.js\";\nimport \"../kit/parts.js\";",
        );
        // A store module may import the kit's state: it is no element or model.
        d.file(
            "host/fun/board/board.state.js",
            "import \"../kit/state.js\";",
        );
        // The stream holds Solid state; a press does not.
        d.file(
            "host/fun/board/wire.js",
            "import \"../kit/press.js\";\nimport \"../kit/stream.js\";",
        );
        d.file("host/fun/board/wire.render.js", "export const w = 1;");
        let max = format!(
            "{}library solid {}\nisland board/log 1000\nisland board/wire 1000\n",
            exact(&d),
            len(&d, "fun/kit/vendor/solid.js")
        );
        assert_eq!(
            check(&host, &max),
            [
                "fun/board/log.js: reaches fun/kit/state.js by its imports; an element or model file loads nothing that draws",
                "fun/board/log.model.js: reaches fun/kit/parts.js by its imports; an element or model file loads nothing that draws",
                "fun/board/wire.js: reaches fun/kit/stream.js by its imports; an element or model file loads nothing that draws",
            ]
        );
        d.file("host/fun/board/wire.js", "import \"../kit/press.js\";");
        d.file(
            "host/fun/board/log.js",
            "import { island } from \"../kit/island.js\";\nisland(\"log\", { load: () => import(\"./log.render.js\") });",
        );
        d.file("host/fun/board/log.model.js", "export const m = 1;");
        assert_eq!(check(&host, &max), Vec::<String>::new());
    }

    /// `small` with the kit: its library, a kit file that draws with it, a
    /// manifest of both, and a view of the business page that imports both.
    fn with_kit(name: &str) -> Dir {
        let d = small(name);
        d.file(
            "host/fun/kit/vendor/solid.js",
            "var a;export{a as For,b as html};",
        );
        d.file(
            "host/fun/kit/vendor/solid.d.ts",
            "export const For: any;\nexport function html(s: TemplateStringsArray): any;\n",
        );
        d.file(
            "host/fun/kit/parts.js",
            "import { html } from \"./vendor/solid.js\";",
        );
        d.file(
            "host/fun/view.js",
            "import { x } from \"./data.js\";\nimport \"./one.js\";\n\
             import { For } from \"./kit/vendor/solid.js\";\nimport \"./kit/parts.js\";",
        );
        let kit = d.0.join("host/fun/kit");
        let line = |f: &str| {
            let bytes = fs::read(kit.join(f)).unwrap();
            let hex: String = sha2::Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            format!("file {hex} {} {f}\n", bytes.len())
        };
        d.file(
            "host/fun/kit/MANIFEST",
            &format!(
                "kit 1\n{}{}{}",
                line("parts.js"),
                line("vendor/solid.d.ts"),
                line("vendor/solid.js")
            ),
        );
        d
    }

    #[test]
    fn the_library_is_counted_once_on_its_own_line_and_in_no_page() {
        let d = with_kit("pages-library-line");
        let host = host_of(&d);
        let s = size(&d, &["fun/business.html"]);
        let script: u64 = [
            "fun/one.js",
            "fun/view.js",
            "fun/data.js",
            "fun/kit/parts.js",
        ]
        .iter()
        .map(|f| len(&d, f))
        .sum();
        assert_eq!(s.script, script, "the library is not in the page");
        let lib = len(&d, "fun/kit/vendor/solid.js");
        // view.js imports the library from a file that is no .render.js.
        let refused = "fun/view.js: imports the library \"./kit/vendor/solid.js\"; only a .render.js file or a kit file that draws may";
        assert_eq!(
            check(&host, &exact(&d)),
            ["library solid: no line in pages.max", refused]
        );
        let max = format!("{}library solid {lib}\n", exact(&d));
        assert_eq!(check(&host, &max), [refused]);
        let max = format!("{}library solid {}\n", exact(&d), lib - 1);
        assert_eq!(
            check(&host, &max)[0],
            format!("library solid: {lib} bytes, the ceiling is {}", lib - 1)
        );
        fs::remove_file(host.join("fun/kit/vendor/solid.js")).unwrap();
        let max = format!("{}library solid {lib}\n", exact(&d));
        assert!(check(&host, &max)
            .contains(&"library solid: in pages.max but no fun/kit/vendor/solid.js".to_string()));
    }

    #[test]
    fn a_kit_file_changed_without_its_manifest_fails() {
        let d = with_kit("pages-kit-manifest");
        let host = host_of(&d);
        d.file(
            "host/fun/view.js",
            "import { x } from \"./data.js\";\nimport \"./one.js\";\nimport \"./kit/parts.js\";",
        );
        let max = format!(
            "{}library solid {}\n",
            exact(&d),
            len(&d, "fun/kit/vendor/solid.js")
        );
        assert_eq!(check(&host, &max), Vec::<String>::new());
        d.file(
            "host/fun/kit/parts.js",
            "import { html } from \"./vendor/solid.js\"; // edited",
        );
        let wrong = check(&host, &max);
        assert!(
            wrong
                .iter()
                .any(|w| w.starts_with("fun/kit/MANIFEST: no line")),
            "{wrong:?}"
        );
    }

    #[test]
    fn a_type_file_is_in_the_manifest_and_in_no_page_and_its_address_is_no_import() {
        let d = with_kit("pages-kit-types");
        let host = host_of(&d);
        d.file(
            "host/fun/kit/vendor/solid.d.ts",
            "export const For: any;\nexport function html(s: TemplateStringsArray): any;\n",
        );
        // A kit module that draws nothing and one that draws name types in the
        // .d.ts from JSDoc, and the business page loads both.
        d.file(
            "host/fun/kit/island.js",
            "/** @import { For } from \"./vendor/solid.d.ts\" */\n\
             /** @typedef {import(\"./vendor/solid.d.ts\").Accessor<number>} N */\n",
        );
        d.file(
            "host/fun/kit/parts.js",
            "import { html } from \"./vendor/solid.js\";\n\
             /** @typedef {import(\"./vendor/solid.d.ts\").Owner} O */\n",
        );
        d.file(
            "host/fun/view.js",
            "import { x } from \"./data.js\";\nimport \"./one.js\";\n\
             import \"./kit/parts.js\";\nimport \"./kit/island.js\";",
        );
        let kit = host.join("fun/kit");
        let line = |f: &str| {
            let bytes = fs::read(kit.join(f)).unwrap();
            let hex: String = sha2::Sha256::digest(&bytes)
                .iter()
                .map(|b| format!("{b:02x}"))
                .collect();
            format!("file {hex} {} {f}\n", bytes.len())
        };
        let manifest = [
            "island.js",
            "parts.js",
            "vendor/solid.d.ts",
            "vendor/solid.js",
        ]
        .map(line)
        .concat();
        d.file("host/fun/kit/MANIFEST", &format!("kit 1\n{manifest}"));

        let script: u64 = [
            "fun/one.js",
            "fun/view.js",
            "fun/data.js",
            "fun/kit/parts.js",
            "fun/kit/island.js",
        ]
        .iter()
        .map(|f| len(&d, f))
        .sum();
        assert_eq!(size(&d, &["fun/business.html"]).script, script);
        let max = format!(
            "{}library solid {}\n",
            exact(&d),
            len(&d, "fun/kit/vendor/solid.js")
        );
        assert_eq!(check(&host, &max), Vec::<String>::new());

        // The manifest names the .d.ts as it names every file of the kit.
        d.file(
            "host/fun/kit/vendor/solid.d.ts",
            "export const For: 1;\nexport function html(): 1;\n",
        );
        let wrong = check(&host, &max);
        assert!(
            wrong
                .iter()
                .any(|w| w.starts_with("fun/kit/MANIFEST: no line") && w.contains("solid.d.ts")),
            "{wrong:?}"
        );
    }

    #[test]
    fn the_imports_of_a_module_leave_out_type_addresses() {
        let text = "/** @import { A } from \"./kit.d.ts\" */\n\
                    /** @type {import('../kit/vendor/solid.d.ts').Owner} */\nimport \"./a.js\";";
        assert_eq!(imports(text), ["./a.js"]);
        assert_eq!(
            imports("import \"./kit.d.ts\";\nconst m = import('./b.d.ts');"),
            ["./kit.d.ts", "./b.d.ts"]
        );
    }

    #[test]
    fn a_type_file_imported_in_code_is_refused() {
        let d = with_kit("pages-kit-type-import");
        let host = host_of(&d);
        d.file(
            "host/fun/one.js",
            "/** @import { For } from \"./kit/vendor/solid.d.ts\" */\n\
             import \"./kit/vendor/solid.d.ts\";\n",
        );
        let max = format!(
            "{}library solid {}\n",
            exact(&d),
            len(&d, "fun/kit/vendor/solid.js")
        );
        assert_eq!(
            check(&host, &max)
                .iter()
                .filter(|w| w.contains("a type file no home has"))
                .collect::<Vec<_>>(),
            ["fun/one.js: imports \"./kit/vendor/solid.d.ts\", a type file no home has; name it in a JSDoc comment"]
        );
    }

    #[test]
    fn a_library_without_its_type_file_is_refused() {
        let d = with_kit("pages-kit-no-types");
        let host = host_of(&d);
        fs::remove_file(host.join(TYPES)).unwrap();
        let kit = host.join("fun/kit/MANIFEST");
        let manifest: String = fs::read_to_string(&kit)
            .unwrap()
            .lines()
            .filter(|l| !l.ends_with("vendor/solid.d.ts"))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(&kit, manifest).unwrap();
        let max = format!(
            "{}library solid {}\n",
            exact(&d),
            len(&d, "fun/kit/vendor/solid.js")
        );
        let wrong = check(&host, &max);
        assert!(
            wrong
                .iter()
                .any(|w| w.starts_with("fun/kit/vendor/solid.d.ts: ")
                    && w.ends_with("; the library has its types")),
            "{wrong:?}"
        );
    }

    #[test]
    fn every_page_the_stage_hashes_is_measured() {
        for staged in STAGED {
            assert!(
                PAGES.iter().any(|(_, files)| files.contains(&staged)),
                "{staged} is not measured"
            );
        }
    }

    /// The real pages against host/fun/pages.max. A page that grew raises its
    /// line in the same change.
    #[test]
    fn the_pages_and_islands_are_under_their_ceilings() {
        let host = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../host");
        let max = fs::read_to_string(host.join("fun/pages.max")).expect("host/fun/pages.max");
        let wrong = check(&host, &max);
        assert!(
            wrong.is_empty(),
            "over host/fun/pages.max (raise a line in the same change only on a person's word):\n{}",
            wrong.join("\n")
        );
    }
}
