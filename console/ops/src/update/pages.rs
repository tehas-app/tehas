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
//! A page or island over its line fails, and so does one with no line or a
//! line with nothing to measure. A change that grows a page raises its line in
//! the same change; a change that shrinks one may lower it. A library import,
//! a bare address or one through a `vendor/` directory, is allowed only in a
//! `.render.js` file.

use super::assets::{address, spans, PAGES as STAGED};
use std::collections::BTreeSet;
use std::fs;
use std::path::{Component, Path, PathBuf};

/// The pages, each with the files of `host` that make it. The dashboard is a
/// fragment the manual's layout renders.
const PAGES: [(&str, &[&str]); 4] = [
    ("business", &["fun/business.html"]),
    ("floor", &["fun/index.html"]),
    ("dashboard", &["docs/_layout.html", "docs/dashboard.frag"]),
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
fn normal(path: &Path) -> Option<PathBuf> {
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
/// `import "x"` and `import("x")`, in the order they stand.
fn imports(text: &str) -> Vec<String> {
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
                if !value[..end].contains('\n') {
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
fn library(spec: &str) -> bool {
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

    for f in &files {
        if f.to_string_lossy().ends_with(".render.js") {
            continue;
        }
        let Ok(text) = fs::read_to_string(host.join(f)) else {
            wrong.push(format!("{}: cannot read", f.display()));
            continue;
        };
        for spec in imports(&text).into_iter().filter(|s| library(s)) {
            wrong.push(format!(
                "{}: imports the library {spec:?}; only a .render.js file may",
                f.display()
            ));
        }
    }
    wrong
}

#[cfg(test)]
mod tests {
    use super::super::testing::{dir, Dir};
    use super::*;

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
            wrong[0].starts_with("pages.max line 5: cannot read"),
            "{wrong:?}"
        );
    }

    #[test]
    fn an_island_needs_a_line_under_the_budget_and_stays_under_it() {
        let d = small("pages-island");
        let host = host_of(&d);
        d.file("host/fun/kit/log.js", "aaaa");
        d.file("host/fun/kit/log.model.js", "bb");
        d.file("host/fun/kit/log.render.js", "c");
        let wrong = check(&host, &exact(&d));
        assert_eq!(wrong, ["island kit/log: no line in pages.max"]);
        assert!(check(&host, &format!("{}island kit/log 7\n", exact(&d))).is_empty());
        let wrong = check(&host, &format!("{}island kit/log 6\n", exact(&d)));
        assert_eq!(wrong, ["island kit/log: 7 bytes, the ceiling is 6"]);
        let wrong = check(&host, &format!("{}island kit/log 12289\n", exact(&d)));
        assert_eq!(
            wrong,
            ["island kit/log: the ceiling 12289 is over the budget of 12288"]
        );
        let wrong = check(
            &host,
            &format!("{}island kit/log 7\nisland gone 1\n", exact(&d)),
        );
        assert_eq!(wrong, ["island gone: in pages.max but no such island"]);
    }

    #[test]
    fn only_a_render_file_may_import_a_library() {
        let d = small("pages-library");
        let host = host_of(&d);
        d.file(
            "host/fun/kit/log.js",
            "import { h } from \"./vendor/preact.js\";",
        );
        d.file("host/fun/kit/log.model.js", "import x from \"lib\";");
        d.file(
            "host/fun/kit/log.render.js",
            "import { h } from \"./vendor/preact.js\";",
        );
        let max = format!("{}island kit/log 1000\n", exact(&d));
        assert_eq!(
            check(&host, &max),
            [
                "fun/kit/log.js: imports the library \"./vendor/preact.js\"; only a .render.js file may",
                "fun/kit/log.model.js: imports the library \"lib\"; only a .render.js file may",
            ]
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
