//! The stage install.sh copies the dashboard's files from.
//!
//!   console update assets <host dir> <stage dir>
//!
//! `host/fun` and `host/docs` are copied to `<stage dir>/fun` and
//! `<stage dir>/docs` as 644. Then each `<script src="/fun/X.js">` and each
//! `<link rel="stylesheet" href="/fun/X.css">` of `fun/index.html`,
//! `fun/business.html` and `docs/dashboard.frag` is named by the bytes it asks
//! for: `fun/X.<hex>.js` holds a copy of `fun/X.js` and the tag names that
//! file, where `<hex>` is the first 12 hex digits of the sha256 of the bytes.
//! The docs server answers such a name `immutable` (host/edge/docs.Caddyfile),
//! so a browser keeps it and asks for the page every time instead.
//!
//! A tag is rewritten only when the file it names is there and holds no
//! template: Caddy reads `{{` as the start of a template, so a name that comes
//! from one - the theme file, named `theme-{{ $design }}.css` - keeps the name
//! it has and is revalidated. A module the page loads by a relative address is
//! left alone too, because the module imports its own siblings by name and a
//! hashed page would ask for files none of them is named by. Nothing else is
//! copied, so a stage holds exactly what a home gets from the checkout, and the
//! sweep of `console update files --sync` (src/update/files.rs) takes back a
//! hashed name whose content changed.
//!
//! The same bytes give the same stage, which is what keeps `--check` quiet on
//! the second run.
//!
//! Exit: 0 staged, 1 a file could not be read or written, 64 usage.

use super::{fault, tell};
use sha2::{Digest, Sha256};
use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

/// The directories of `<host dir>` that go to the stage, by their own name.
const DIRS: [&str; 2] = ["fun", "docs"];

/// The pages that name a script or a stylesheet by tag, from the stage.
const PAGES: [&str; 3] = ["fun/index.html", "fun/business.html", "docs/dashboard.frag"];

/// The extensions a page may name by tag and the server may cache for ever.
/// Nothing else is given a hashed name: an image, a font or a document keeps
/// its name and is revalidated, as the theme file does.
const HASHED: [&str; 2] = ["js", "css"];

/// The name of an asset's copy: `<stem>.<hex>.<extension>`, `<hex>` the first
/// 12 hex digits of the sha256 of its bytes. The stem is the whole name before
/// the extension, so the hex sits between the name and its type.
fn hashed_name(name: &str, bytes: &[u8]) -> String {
    let path = Path::new(name);
    let extension = path.extension().map(|e| e.to_string_lossy().into_owned());
    // A name with no dot in it has no extension: the hex goes at the end.
    let stem = match extension {
        Some(ref e) => {
            let whole = path.file_name().map(|n| n.to_string_lossy().into_owned());
            match whole {
                Some(w) => w[..w.len() - e.len() - 1].to_string(),
                None => String::new(),
            }
        }
        None => path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    };
    let hex: String = Sha256::digest(bytes)
        .iter()
        .take(6)
        .map(|byte| format!("{byte:02x}"))
        .collect();
    match extension {
        Some(e) => format!("{stem}.{hex}.{e}"),
        None => format!("{stem}.{hex}"),
    }
}

/// The attribute a tag names its address with, and the address: from a tag with
/// `src="/fun/…"` or `href="/fun/…"`. Nothing when the tag names no address
/// under `/fun/` - a relative address among them, which is how a module names
/// the modules it imports. `src=` wins where a tag holds both, because that is
/// the attribute that names what a script runs.
fn address(tag: &str) -> Option<(&'static str, &str)> {
    // Past the attribute, the bytes up to the first `"` are nothing, the ones
    // after it are the value, and the value ends at the first `"` or the first
    // `>`: a tag's last attribute may be closed by the `>` that ends the tag.
    let value = |attribute: &'static str| {
        let rest = &tag[tag.find(attribute)? + attribute.len()..];
        let open = rest.strip_prefix('"')?;
        Some(&open[..open.find(['"', '>']).unwrap_or(open.len())])
    };
    if let Some(addr) = value("src=").filter(|a| a.starts_with("/fun/")) {
        return Some(("src=", addr));
    }
    let addr = value("href=").filter(|a| a.starts_with("/fun/"))?;
    Some(("href=", addr))
}

/// The path an address names, under the stage. Nothing when it names a parent:
/// the address of a tag of these pages names a file of `fun` and no other.
fn asset_path(stage: &Path, addr: &str) -> Option<PathBuf> {
    let relative = Path::new(addr).strip_prefix("/").ok()?;
    if relative
        .components()
        .any(|c| matches!(c, std::path::Component::ParentDir))
    {
        return None;
    }
    Some(stage.join(relative))
}

/// The `<script …>` and `<link …>` tags of `text`, as byte spans of it, in the
/// order they stand. Each starts at a `<` followed by a letter and ends at the
/// `>` that closes it, which for a `<script>` is the one that closes it and not
/// a `>` of its own text: the search for it starts past the opening quote of an
/// attribute, so a `>` written inside one cannot end the tag.
fn spans(text: &str) -> Vec<(usize, usize)> {
    let mut found = Vec::new();
    let mut at = 0;
    while let Some(offset) = text[at..].find('<') {
        let open = at + offset;
        // A `<` that opens no tag - text in a script, a stray one in prose - is
        // no tag, and the search goes on from inside it rather than past it.
        if !text[open + 1..]
            .chars()
            .next()
            .is_some_and(|c| c.is_alphabetic())
        {
            at = open + 1;
            continue;
        }
        let Some(closing) = close(text, open) else {
            break;
        };
        let tag = &text[open..closing];
        if tag.starts_with("<script ") || tag.starts_with("<link ") {
            found.push((open, closing));
        }
        at = closing;
    }
    found
}

/// Where the tag that opens at `open` ends: just past the first `>` of it that
/// stands outside the quoted values of its attributes. Nothing when no `>`
/// closes it.
fn close(text: &str, open: usize) -> Option<usize> {
    let mut from = open + 1;
    let mut quoted = false;
    while let Some(offset) = text[from..].find(['"', '>']) {
        let at = from + offset;
        if text.as_bytes()[at] == b'"' {
            quoted = !quoted;
        } else if !quoted {
            return Some(at + 1);
        }
        from = at + 1;
    }
    None
}

/// The span of the `n`-th tag of a page, or nothing where the page holds no
/// more tags.
fn next_tag(text: &str, n: usize) -> Option<(usize, usize)> {
    spans(text).into_iter().nth(n)
}

/// One page: every tag of it rewritten to the hashed name of the file it names.
/// A file that is not in the stage, or that holds a template, leaves its tag as
/// it is, and the server answers that name as it always did.
fn page(stage: &Path, name: &str) -> io::Result<()> {
    let path = stage.join(name);
    let mut text = fs::read_to_string(&path)?;
    let mut rewrites = 0;
    // Each tag in turn, and the rewrite that answers for it: two tags of one
    // page may name one file, so a tag is replaced where it stands and no
    // other tag of the page is touched. The page is read again each time, so a
    // rewrite that changed a span's length is no reason to lose the next tag.
    while let Some(start) = next_tag(&text, rewrites) {
        let Some((attribute, addr)) = address(&text[start.0..start.1]) else {
            rewrites += 1;
            continue;
        };
        let Some(source) = asset_path(stage, addr) else {
            rewrites += 1;
            continue;
        };
        // Caddy reads `{{` as the start of a template, so a file whose bytes
        // carry one names something no file of this checkout can answer for:
        // no name to hash, and the tag is left as it is.
        let Some(bytes) = fs::read(&source)
            .ok()
            .filter(|bytes| !bytes.windows(2).any(|w| w == b"{{"))
        else {
            rewrites += 1;
            continue;
        };
        // The server's matcher of the immutable header names these two
        // extensions and no other: a hashed name of another type would be
        // cached for ever by nothing, so the tag keeps its name.
        let extension = source
            .extension()
            .map(|e| e.to_string_lossy().into_owned())
            .unwrap_or_default();
        if !HASHED.contains(&extension.as_str()) {
            rewrites += 1;
            continue;
        }
        // The name is the file's own, not its path: the extension is what
        // comes after the last dot of the name, and a path would carry the
        // separators of the directories above it.
        let hashed = hashed_name(
            &source.file_name().unwrap_or_default().to_string_lossy(),
            &bytes,
        );
        let copy = source.parent().unwrap_or(Path::new("")).join(&hashed);
        fs::write(&copy, &bytes)?;
        fs::set_permissions(&copy, fs::Permissions::from_mode(0o644))?;
        let quoted = format!("{attribute}\"{addr}\"");
        let at = text[start.0..start.1].find(&quoted).map(|at| start.0 + at);
        if let Some(at) = at {
            text.replace_range(
                at..at + quoted.len(),
                &format!("{attribute}\"/fun/{hashed}\""),
            );
        }
        rewrites += 1;
    }
    if rewrites != 0 {
        fs::write(&path, text)?;
    }
    Ok(())
}

/// One directory of the host, whole, into the stage as 644. A name that begins
/// with a dot is no page and no asset, so it is not copied.
fn copy_dir(from: &Path, to: &Path) -> io::Result<()> {
    let mut entries: Vec<_> = fs::read_dir(from)?
        .filter_map(Result::ok)
        .filter(|e| !e.file_name().to_string_lossy().starts_with('.'))
        .collect();
    entries.sort_by_key(|e| e.file_name());
    fs::create_dir_all(to)?;
    for entry in entries {
        let name = entry.file_name().to_string_lossy().into_owned();
        let source = entry.path();
        if source.is_dir() {
            copy_dir(&source, &to.join(&name))?;
            continue;
        }
        let bytes = fs::read(&source)?;
        let placed = to.join(&name);
        fs::write(&placed, &bytes)?;
        fs::set_permissions(&placed, fs::Permissions::from_mode(0o644))?;
    }
    Ok(())
}

/// `console update assets <host dir> <stage dir>`. It prints nothing: it is one
/// step of an install, which says what it did of its own, and `--check` reads
/// the lines of the report alone.
pub fn verb(given: &[String]) -> u8 {
    let [host, stage] = given else {
        tell("console update assets: usage: <host dir> <stage dir>");
        return 64;
    };
    let host = Path::new(host);
    let stage = Path::new(stage);
    let staged =
        copy_dirs(host, stage).and_then(|()| PAGES.iter().try_for_each(|name| page(stage, name)));
    match staged {
        Ok(_) => 0,
        Err(why) => {
            fault(&format!("could not stage the dashboard's files: {why}"));
            1
        }
    }
}

/// `host/fun` and `host/docs`, whole, under the stage.
fn copy_dirs(host: &Path, stage: &Path) -> io::Result<()> {
    DIRS.iter()
        .try_for_each(|name| copy_dir(&host.join(name), &stage.join(name)))
}

#[cfg(test)]
mod tests {
    use super::super::testing::dir;
    use super::*;

    /// A host directory with the three pages and the files they name. Each file
    /// holds exactly the text written here: the console's own test helper adds a
    /// newline to what it writes, which would end these tags early.
    fn host(name: &str) -> super::super::testing::Dir {
        let d = dir(name);
        put(
            &d,
            "host/fun/index.html",
            "<html>\n  <link rel=\"stylesheet\" href=\"/fun/panels.css\">\n  \
             <link rel=\"stylesheet\" href=\"/fun/theme-{{ $design }}.css\">\n  \
             <script src=\"/fun/panels.js\"></script>\n</html>\n",
        );
        put(&d, "host/fun/panels.css", ".panel { color: green }\n");
        put(&d, "host/fun/panels.js", "// the panel\n");
        put(&d, "host/fun/theme-pixel.css", "/* pixel */\n");
        put(&d, "host/fun/visible.js", "// staged\n");
        put(&d, "host/fun/.hidden.js", "// not staged\n");
        put(
            &d,
            "host/fun/business.html",
            "<link rel=\"stylesheet\" href=\"/fun/panels.css\">\n\
             <script src=\"/fun/panels.js\"></script>\n",
        );
        put(
            &d,
            "host/docs/dashboard.frag",
            "<script src=\"/fun/panels.js\"></script>\n",
        );
        put(&d, "host/docs/host.md", "# The host\n");
        d
    }

    /// A file of a test's own, holding `text` byte for byte.
    fn put(d: &super::super::testing::Dir, name: &str, text: &str) {
        let path = d.0.join(name);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, text).unwrap();
    }

    fn run(d: &super::super::testing::Dir) -> u8 {
        verb(&[
            d.0.join("host").to_string_lossy().into_owned(),
            d.0.join("stage").to_string_lossy().into_owned(),
        ])
    }

    fn staged(d: &super::super::testing::Dir, name: &str) -> PathBuf {
        d.0.join("stage").join(name)
    }

    fn names(dir: &Path) -> Vec<String> {
        let mut out: Vec<String> = fs::read_dir(dir)
            .unwrap()
            .filter_map(Result::ok)
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect();
        out.sort();
        out
    }

    /// The first 12 hex digits of a file's sha256, the middle of its copy's name.
    fn hex(bytes: &[u8]) -> String {
        Sha256::digest(bytes)
            .iter()
            .take(6)
            .map(|b| format!("{b:02x}"))
            .collect()
    }

    #[test]
    fn a_tag_is_rewritten_to_the_hashed_name_and_the_copy_is_staged() {
        let d = host("assets-rewrite");
        assert_eq!(run(&d), 0);
        let js = b"// the panel\n";
        let css = b".panel { color: green }\n";
        let js_name = format!("panels.{}.js", hex(js));
        let css_name = format!("panels.{}.css", hex(css));
        let page = fs::read_to_string(staged(&d, "fun/index.html")).unwrap();
        assert!(
            page.contains(&format!("<script src=\"/fun/{js_name}\"></script>")),
            "{page}"
        );
        assert!(
            page.contains(&format!(
                "<link rel=\"stylesheet\" href=\"/fun/{css_name}\">"
            )),
            "{page}"
        );
        // Neither page asks for the plain name any more.
        assert!(!page.contains("\"/fun/panels.js\""), "{page}");
        assert!(names(&staged(&d, "fun")).contains(&js_name));
        assert!(names(&staged(&d, "fun")).contains(&css_name));
        // The copy holds the bytes it was named for, and the page that names it
        // keeps its own name.
        assert_eq!(
            fs::read(staged(&d, &format!("fun/panels.{}.js", hex(js)))).unwrap(),
            js
        );
        let frag = fs::read_to_string(staged(&d, "docs/dashboard.frag")).unwrap();
        assert!(frag.contains(&format!("/fun/{js_name}")), "{frag}");
        let business = fs::read_to_string(staged(&d, "fun/business.html")).unwrap();
        assert!(business.contains(&format!("/fun/{js_name}")), "{business}");
        assert!(business.contains(&format!("/fun/{css_name}")), "{business}");
    }

    #[test]
    fn the_theme_link_the_template_names_is_left_as_it_is() {
        let d = host("assets-theme");
        assert_eq!(run(&d), 0);
        let page = fs::read_to_string(staged(&d, "fun/index.html")).unwrap();
        assert!(
            page.contains("<link rel=\"stylesheet\" href=\"/fun/theme-{{ $design }}.css\">"),
            "{page}"
        );
        // The theme file itself is staged as it is, under no hashed name, and no
        // copy of it carries the bytes of a template.
        assert_eq!(
            fs::read_to_string(staged(&d, "fun/theme-pixel.css")).unwrap(),
            "/* pixel */\n"
        );
        assert!(names(&staged(&d, "fun")).contains(&"theme-pixel.css".to_string()));
        for name in names(&staged(&d, "fun")) {
            assert!(!name.starts_with("theme-."), "{name}");
        }
    }

    #[test]
    fn changed_content_gives_a_new_name() {
        let d = host("assets-change");
        assert_eq!(run(&d), 0);
        let before = names(&staged(&d, "fun"));
        let old = format!("panels.{}.js", hex(b"// the panel\n"));
        assert!(before.contains(&old), "{before:?}");

        // The bytes change, so the name every page asks for changes with them.
        fs::write(d.0.join("host/fun/panels.js"), "// another panel\n").unwrap();
        fs::remove_dir_all(d.0.join("stage")).unwrap();
        assert_eq!(run(&d), 0);
        let after = names(&staged(&d, "fun"));
        let new = format!("panels.{}.js", hex(b"// another panel\n"));
        assert!(after.contains(&new), "{after:?}");
        assert!(!after.contains(&old), "{after:?}");
        let page = fs::read_to_string(staged(&d, "fun/index.html")).unwrap();
        assert!(page.contains(&format!("/fun/{new}")), "{page}");
        assert!(!page.contains(&old), "{page}");
    }

    #[test]
    fn the_same_input_gives_the_same_stage() {
        let first = host("assets-same");
        assert_eq!(run(&first), 0);
        let second = host("assets-same");
        assert_eq!(run(&second), 0);
        assert_eq!(
            names(&staged(&first, "fun")),
            names(&staged(&second, "fun"))
        );
        assert_eq!(
            fs::read_to_string(staged(&first, "fun/index.html")).unwrap(),
            fs::read_to_string(staged(&second, "fun/index.html")).unwrap()
        );
        // Staging the same tree twice over is no different: nothing here reads
        // a clock, which is what keeps --check quiet the second time.
        assert_eq!(run(&first), 0);
        assert_eq!(
            fs::read_to_string(staged(&first, "fun/index.html")).unwrap(),
            fs::read_to_string(staged(&second, "fun/index.html")).unwrap()
        );
    }

    #[test]
    fn a_tag_that_names_nothing_the_stage_holds_is_left_as_it_is() {
        let d = dir("assets-missing");
        put(
            &d,
            "host/fun/index.html",
            "<script src=\"/fun/gone.js\"></script>\n\
             <script type=\"module\" src=\"./panels.js\"></script>\n\
             <script type=\"module\" src=\"/fun/dashboard-view.js\"></script>\n\
             <link rel=\"icon\" href=\"/fun/favicon.svg\">\n\
             <link rel=\"stylesheet\" href=\"/fun/manual.css\">\n",
        );
        put(&d, "host/fun/business.html", "<p>nothing</p>\n");
        put(&d, "host/fun/dashboard-view.js", "import \"./dom.js\";\n");
        put(&d, "host/fun/favicon.svg", "<svg></svg>\n");
        put(&d, "host/fun/manual.css", "body { margin: 0 }\n");
        put(&d, "host/docs/dashboard.frag", "<p>nothing</p>\n");
        put(&d, "host/docs/host.md", "# The host\n");
        assert_eq!(run(&d), 0);
        let page = fs::read_to_string(staged(&d, "fun/index.html")).unwrap();
        // A file the stage does not hold, and a relative address a module names
        // its siblings by, keep their tags.
        assert!(
            page.contains("<script src=\"/fun/gone.js\"></script>"),
            "{page}"
        );
        assert!(page.contains("src=\"./panels.js\""), "{page}");
        assert!(page.contains("href=\"/fun/favicon.svg\""), "{page}");
        // A stylesheet is hashed; an image of the same directory is not, though
        // both are named by a tag the stage can answer for.
        assert!(
            page.contains(&format!("/fun/manual.{}.css", hex(b"body { margin: 0 }\n"))),
            "{page}"
        );
        assert!(page.contains("href=\"/fun/favicon.svg\""), "{page}");
        // A file the stage does hold is named by its hashed copy, and the file
        // itself is staged as it is, imports and all.
        assert!(
            page.contains(&format!(
                "/fun/dashboard-view.{}.js",
                hex(b"import \"./dom.js\";\n")
            )),
            "{page}"
        );
        assert_eq!(
            fs::read_to_string(staged(&d, "fun/dashboard-view.js")).unwrap(),
            "import \"./dom.js\";\n"
        );
    }

    #[test]
    fn a_file_of_the_host_is_staged_and_no_hidden_name_comes_with_it() {
        let d = host("assets-copy");
        assert_eq!(run(&d), 0);
        assert_eq!(
            fs::read_to_string(staged(&d, "docs/host.md")).unwrap(),
            "# The host\n"
        );
        assert!(names(&staged(&d, "fun")).contains(&"panels.css".to_string()));
        assert!(names(&staged(&d, "fun")).contains(&"visible.js".to_string()));
        for name in names(&staged(&d, "fun")) {
            assert!(!name.starts_with('.'), "{name}");
        }
        assert_eq!(mode(&staged(&d, "fun/panels.css")), 0o644);
    }

    fn mode(path: &Path) -> u32 {
        fs::metadata(path).unwrap().permissions().mode() & 0o7777
    }

    #[test]
    fn a_missing_directory_or_page_is_refused_with_1() {
        let d = dir("assets-nofun");
        put(&d, "host/docs/host.md", "# The host\n");
        assert_eq!(run(&d), 1);
        let d = dir("assets-nopage");
        put(&d, "host/fun/panels.js", "// the panel\n");
        put(&d, "host/docs/host.md", "# The host\n");
        assert_eq!(run(&d), 1);
    }

    #[test]
    fn the_arguments_are_two_directories() {
        assert_eq!(verb(&[]), 64);
        assert_eq!(verb(&["a".to_string()]), 64);
        assert_eq!(
            verb(&["a".to_string(), "b".to_string(), "c".to_string()]),
            64
        );
    }

    #[test]
    fn a_name_is_the_stem_the_hex_and_the_extension() {
        assert_eq!(
            hashed_name("panels.js", b"// the panel\n"),
            format!("panels.{}.js", hex(b"// the panel\n"))
        );
        // A dotted stem keeps every part of it before the extension.
        assert_eq!(
            hashed_name("console-tabs.min.css", b"x"),
            format!("console-tabs.min.{}.css", hex(b"x"))
        );
        // A name with no extension gets the hex at the end.
        assert_eq!(
            hashed_name("LICENSE", b"x"),
            format!("LICENSE.{}", hex(b"x"))
        );
        // Two names of one stem, bytes apart, are two names apart.
        assert_ne!(hashed_name("a.js", b"one"), hashed_name("a.js", b"two"));
    }

    #[test]
    fn only_a_tag_that_names_an_address_under_fun_is_rewritten() {
        // The tag is asked the question first: is there an address under /fun/
        // here, and which attribute names it.
        assert_eq!(
            address("<script src=\"/fun/panels.js\">"),
            Some(("src=", "/fun/panels.js"))
        );
        assert_eq!(
            address("<link rel=\"stylesheet\" href=\"/fun/panels.css\">"),
            Some(("href=", "/fun/panels.css"))
        );
        // A relative address, an address of another page's, a tag with no
        // address, and a tag that is no script or link.
        assert_eq!(address("<script type=\"module\" src=\"./dom.js\">"), None);
        assert_eq!(address("<link rel=\"canonical\" href=\"/business\">"), None);
        assert_eq!(address("<link rel=\"stylesheet\">"), None);
        assert_eq!(address("<p>hello</p>"), None);
        // src= is the address a script runs, so it is the one rewritten where a
        // tag holds both.
        assert_eq!(
            address("<script src=\"/fun/a.js\" data-fallback=\"/fun/b.js\">"),
            Some(("src=", "/fun/a.js"))
        );
        assert_eq!(
            address("<link href=\"/fun/a.css\" data-alt=\"/fun/b.css\">"),
            Some(("href=", "/fun/a.css"))
        );
    }

    /// The tags of a page, as text: what `spans` points at.
    fn tags(text: &str) -> Vec<String> {
        spans(text)
            .into_iter()
            .map(|(from, to)| text[from..to].to_string())
            .collect()
    }

    #[test]
    fn the_tags_of_a_page_are_its_scripts_and_links() {
        // A tag is the tag itself: from its `<` to the `>` that closes it, not
        // to the end of the element it opens. What follows the tag is read for
        // the tags in it, not taken as part of it.
        assert_eq!(
            tags("<p><script src=\"a\"></script><link href=\"b\"><em>c</em></p>"),
            vec!["<script src=\"a\">", "<link href=\"b\">"]
        );
        // A script whose text holds a `<` is one tag, and the script after it is
        // still found: the tag ends at the `>` that closes it, not at one of its
        // own text.
        assert_eq!(
            tags("<script>if (a < b) { }</script><script src=\"/fun/a.js\"></script>"),
            vec!["<script src=\"/fun/a.js\">"]
        );
        // A tag that never closes gives nothing, and so does text with no tag.
        assert_eq!(tags("<script src=\"a\""), Vec::<String>::new());
        assert_eq!(tags("<p>no tags here</p>"), Vec::<String>::new());
        // Only scripts and links: a page's other tags are none of this verb's.
        assert_eq!(tags("<em>hi</em><b>there</b>"), Vec::<String>::new());
    }
}
