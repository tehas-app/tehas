//! The marker reader held to a CommonMark and GFM parser, comrak, as the
//! oracle of where GitHub shows a marker. comrak is a dev-dependency of
//! this crate alone; nothing outside its tests uses it.
//!
//! The classes, from comrak's tree, for each marker in a body:
//! - `live`: its words are an HTML block or inline HTML at the text's
//!   level or in a list item, which GitHub keeps as hidden HTML;
//! - `code`: inside a code block or a code span;
//! - `quote`: inside a block quote, at any depth;
//! - `text`: shown as words (comrak read no HTML there): an escaped
//!   `<!--`, a link's or an image's destination or title;
//! - `none`: nowhere in the tree (a table dropped it with its row's extra
//!   cells, a link reference definition took it).
//!
//! Each marker is asked about alone: its body with every other marker made
//! no marker of the same shape, so one marker's answer never hides
//! another's. The reader must never read a marker that is `code`, `quote`
//! or `text` (the unsafe way: no difference allowed); a `none` marker is
//! skipped. It must read every `live` marker, except in the known safe
//! departures, listed by body in [`SAFE`].
//!
//! The corpus is recorded in `tests/samples/markers-oracle.tsv`, each
//! body with the classes comrak gave, so a comrak upgrade that moves a
//! class shows as a difference here. `TEHAS_RECORD_ORACLE=1` rewrites it
//! from comrak.

use comrak::nodes::{AstNode, NodeValue};
use comrak::{parse_document, Arena, Options};
use std::path::PathBuf;
use tehas_github::markers::read;
use tehas_ports::{BlueprintNo, Marker, MarkerKind};

/// The marker the corpus writes for `M`.
const M: &str = "<!-- tehas:finish issue=7 result=opened -->";
/// The words comrak and the reader look for.
const WORDS: &str = "tehas:finish";
/// The same words of no marker: a copy of `M` that keeps the text's shape.
const OTHER: &str = "tehaz:finish";

/// The comrak this corpus was recorded with.
const ORACLE: &str =
    "comrak 0.28.0, default features off, GFM table, strikethrough, autolink and tasklist on";

/// Bodies where the reader does not read a marker GitHub shows as hidden
/// HTML: the safe way, each named for why.
const SAFE: &[(&str, &str)] = &[
    (
        "x <a b=\"`\"> M `",
        "a backtick in an HTML tag's attribute and one after the marker: the reader pairs no runs, so the marker between is no marker",
    ),
    (
        "| a | b |\\n|---|---|\\n| `x | M` |",
        "a table's cells split the row before its code spans pair: the reader reads the row as one line",
    ),
    (
        "- [a]: http://x\\n  x\\n\\n\\n    M",
        "an item's paragraph that opens as a link reference definition is taken for definitions alone, so the second blank line closes the item, and the indented line is taken for code",
    ),
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Live,
    Code,
    Quote,
    Text,
    None,
}

impl Class {
    fn word(self) -> &'static str {
        match self {
            Class::Live => "live",
            Class::Code => "code",
            Class::Quote => "quote",
            Class::Text => "text",
            Class::None => "none",
        }
    }

    fn of(word: &str) -> Class {
        match word {
            "live" => Class::Live,
            "code" => Class::Code,
            "quote" => Class::Quote,
            "text" => Class::Text,
            "none" => Class::None,
            other => panic!("no class {other}"),
        }
    }
}

/// The classes comrak gives the markers of `body`, in their order.
fn oracle(body: &str) -> Vec<Class> {
    // GFM's tagfilter is left off: it changes how HTML is written out, not
    // the tree.
    let mut o = Options::default();
    o.extension.table = true;
    o.extension.strikethrough = true;
    o.extension.autolink = true;
    o.extension.tasklist = true;
    let arena = Arena::new();
    let root = parse_document(&arena, body, &o);
    let mut out = Vec::new();
    walk(root, false, &mut out);
    out
}

fn walk<'a>(n: &'a AstNode<'a>, quoted: bool, out: &mut Vec<Class>) {
    let value = &n.data.borrow().value;
    let quoted = quoted || matches!(value, NodeValue::BlockQuote);
    let (literal, class) = match value {
        // A fence's info words are the code block's too.
        NodeValue::CodeBlock(c) => (Some(format!("{} {}", c.info, c.literal)), Class::Code),
        NodeValue::Code(c) => (Some(c.literal.clone()), Class::Code),
        NodeValue::HtmlBlock(h) => (Some(h.literal.clone()), Class::Live),
        NodeValue::HtmlInline(t) => (Some(t.clone()), Class::Live),
        NodeValue::Text(t) => (Some(t.clone()), Class::Text),
        // A marker in a link's or an image's address or title is shown, or
        // lost, but never hidden HTML.
        NodeValue::Link(l) | NodeValue::Image(l) => {
            (Some(format!("{} {}", l.url, l.title)), Class::Text)
        }
        _ => (None, Class::Text),
    };
    if let Some(l) = literal {
        let class = if quoted { Class::Quote } else { class };
        out.extend(l.matches(WORDS).map(|_| class));
    }
    for c in n.children() {
        walk(c, quoted, out);
    }
}

/// One body per marker of `body`, in their order: the marker, and every
/// other made no marker of the same shape, which keeps the blocks.
fn alone(body: &str) -> Vec<String> {
    body.match_indices(WORDS)
        .map(|(i, _)| {
            let mut one = body.replace(WORDS, OTHER);
            one.replace_range(i..i + WORDS.len(), WORDS);
            one
        })
        .collect()
}

/// The class comrak gives each marker of `body`, each asked alone.
fn classes(body: &str) -> Vec<Class> {
    alone(body)
        .iter()
        .map(|one| match oracle(one).as_slice() {
            [] => Class::None,
            // A link reference definition's destination is copied into
            // each link that uses it: the least hidden copy counts.
            more => *more.iter().find(|c| **c != Class::Live).unwrap_or(&more[0]),
        })
        .collect()
}

/// Whether the reader reads each marker of `body`, each read alone.
fn reader(body: &str) -> Vec<bool> {
    alone(body)
        .iter()
        .map(|one| {
            read(one, BlueprintNo::new(7))
                .markers
                .iter()
                .any(|m| m.kind() == MarkerKind::Finish && matches!(m, Marker::Finish(_)))
        })
        .collect()
}

/// A marker the reader reads where GitHub shows no hidden HTML.
fn unsafe_read(live: bool, class: Class) -> bool {
    live && !matches!(class, Class::Live | Class::None)
}

/// A body as the corpus file writes it: `M` the marker, and `\\`, `\n`,
/// `\r`, `\t`, `\u00a0` and `\ufeff` escaped. A backslash before any other
/// character stands for itself.
fn unescape(s: &str) -> String {
    let mut out = String::new();
    let mut chars = s.chars();
    while let Some(c) = chars.next() {
        if c == 'M' {
            out.push_str(M);
            continue;
        }
        if c != '\\' {
            out.push(c);
            continue;
        }
        match chars.clone().next() {
            Some('\\') => out.push('\\'),
            Some('n') => out.push('\n'),
            Some('r') => out.push('\r'),
            Some('t') => out.push('\t'),
            Some('u') => {
                let hex: String = chars.clone().skip(1).take(4).collect();
                out.push(char::from_u32(u32::from_str_radix(&hex, 16).unwrap()).unwrap());
                chars.nth(3);
            }
            _ => {
                out.push('\\');
                continue;
            }
        }
        chars.next();
    }
    out
}

fn escape(s: &str) -> String {
    s.replace('\\', "\\\\")
        .replace(M, "M")
        .replace('\n', "\\n")
        .replace('\r', "\\r")
        .replace('\t', "\\t")
        .replace('\u{a0}', "\\u00a0")
        .replace('\u{feff}', "\\ufeff")
}

/// The bodies chosen by hand: the review's probes, the factory's shapes
/// and 1.x's, each written with `M`.
const CHOSEN: &[&str] = &[
    // The factory's and 1.x's shapes: each must read.
    "M",
    "text\\n\\nM",
    "M\\ntext after",
    "text\\nM",
    "# h\\nM",
    "```\\nx\\n```\\nM",
    "<details>\\n<summary>s</summary>\\n\\nbody\\n\\n</details>\\n\\nM",
    "- a\\n- b\\n\\nM",
    "| a |\\n|---|\\n| 1 |\\n\\nM",
    "text\\r\\n\\r\\nM",
    "M\\r\\ntext",
    "text\\r\\nM",
    "> q\\n\\nM",
    "    code\\n\\nM",
    "- a\\n\\n      code\\n\\nM",
    "Said:\\n> quoted\\n\\nM\\nM",
    "M\\n    indented first line\\nmore",
    // The review's probes.
    "> quoted\\nM",
    "> quoted\\n M",
    "> quoted\\n  M",
    "> quoted\\n   M",
    "> quoted\\n    M",
    "> quoted\\nlazy M",
    "> - item\\nM",
    "> - item\\nlazy M",
    "- item\\nM",
    "- item\\n  M",
    "- item\\n\\n  M",
    "- M",
    "1. item\\n\\n       M",
    "- item\\n\\n      M",
    "- item\\n\\n    M",
    "para\\n\\n\\tM",
    "   M",
    "    M",
    "<details>\\n\\n    M",
    "<details>\\n    M",
    "| a | b |\\n|---|---|\\n| 1 | 2 |\\nM",
    "| a | b |\\n|---|---|\\n| 1 | 2 |\\n    M",
    "text\\n===\\n    M",
    "text\\n---\\n    M",
    "M\\n    M",
    "> ```\\n> x\\n> ```\\nM",
    "> ```\\n> x\\n> ```\\n    M",
    "> ```\\n> x\\nlazy M",
    ">     code\\nlazy M",
    "> quoted\\n\\n> more\\nM",
    ">\\nM",
    "> quoted\\n>\\n    M",
    "> quoted\\r\\nM",
    "> quoted\\r\\nlazy M",
    "para\\r\\n\\r\\n    M",
    "> quoted\\n   \\nM",
    "> quoted\\n   \\nlazy M",
    "> quoted\\n2. item M",
    "> quoted\\n1. item M",
    "- > M",
    "- > quoted\\n  lazy M",
    "- ```\\n  M\\n  ```",
    "- item\\n\\n    ```\\n    M\\n    ```",
    "- item\\n# h\\n\\n    M",
    "- item\\n```\\nx\\n```\\n\\n    M",
    "- item\\nM\\n    M",
    "1. item\\n\\n  para\\n\\n    M",
    "<!-- a\\nb -->\\n    M",
    "<!-- a --> tail\\n    M",
    "<pre>x</pre>\\n    M",
    "para\\r\\r    M",
    "x\\r> M",
    "> quoted\\n \\nlazy M",
    "> quoted\\n\\u00a0\\nlazy M",
    "para\\n \\n    M",
    ">\\n> q\\nM",
    "- item\\n\\n\\tM",
    "para\\n    M",
    "- item\\n\\npara\\n\\n    M",
    "a `x\\nb M `",
    "See `M` here.",
    "\\\\\\\\`M\\\\\\\\`",
    // The second read's causes: partly used tabs, empty items, the first
    // kind of HTML block's close tags, backticks GitHub reads as something
    // else first, and a byte order mark.
    "-\\t  M",
    "1.\\t   M",
    "- a\\n\\n\\t  M",
    "1. a\\n\\n\\t   M",
    "-\\n\\n    M",
    "1.\\n\\n    M",
    "-   \\n\\n    M",
    "<pre>\\n</script>\\n\\n    M",
    "<pre>\\n</style>\\n\\n    M",
    "<pre>\\n</textarea>\\n\\n    M",
    "<script>\\n</pre>\\n\\n    M",
    "<textarea>\\n</pre>\\n\\n    M",
    "<style>\\n</script>\\n\\n    M",
    "x <a title='`'> `M`",
    "x <http://a.b/`> `M`",
    "x <!-- ` --> `M`",
    "x [a](`) `M`",
    "x <?`?> `M`",
    "x <!X `> `M`",
    "x <![CDATA[`]]> `M`",
    "x <a\\nb='`'> `M`",
    "[a]: /u '`'\\n`M`",
    "[a]: </u`>\\n`M`",
    "[a]: /u\\n'`'\\n`M`",
    "\\ufeff    M",
    // The safe departures, and a marker inside a larger comment, which is
    // hidden HTML to both.
    "x <a b=\"`\"> M `",
    "| a | b |\\n|---|---|\\n| `x | M` |",
    "<!--\\nM\\n-->",
    // Found unsafe by seeded random runs while this was built: a close tag
    // of the first kind is of the seventh, a lone tag must be whole, and a
    // table's head must have as many cells as its delimiter row.
    "\\n\\n</pre>\\n- |---|---|\\n\\n\\t- <div>M",
    "</pre>  \\n<pre>| a | b |\\n\\n>M",
    "</pre>\\r\\n- | a | b |- ===``\\n\\n\\t`Mtext        ",
    "   </pre>\\r<pre>            ---\\n\\n\\t|---|---|1.-M",
    "  10) </pre>\\n\\t> - M",
    "</pre><!--```-\\r\\n|---|---|\\n</pre>\\n~~~`1. M",
    "   </pre>> \\n> |---|---|> ---</pre>\\tM",
    "</pre>---```  -->-->\\n> </div>M",
    "</pre>  -->\\n```\\n\\n# > M",
    // The lead's read: a table whose head is a paragraph's last line (more
    // in [`tables`]), an escaped opener and a marker in a link's
    // destination or title, which GitHub shows as words, and nested
    // containers on one line.
    "p\\na | b\\n-|-\\n    M",
    "p\\nx\\n:-\\n    M",
    "p\\n<\\n:-\\n    M",
    "\\\\M",
    "x \\\\M",
    "x \\\\\\\\M",
    "x \\\\\\\\\\\\M",
    "# \\\\M",
    "| a |\\n|---|\\n| \\\\M |",
    "<div>\\n\\\\M",
    "[a](M)",
    "[a](</u> \"M\")",
    "[a](/u 'M')",
    "[a](/u (M))",
    "![i](M)",
    "x [a](\\nM\\n)",
    "[a](M title)",
    "[a](M",
    "[M](/u)",
    "[a]: M",
    "[a]: /u \"M\"",
    "[a]: /u\\n\"M\"",
    "[a]:\\nM",
    "- - - M",
    "> - > - M",
    "1. - > 2) M",
    // Found unsafe by the seeds of the lead's read: an item whose first
    // line opens another item has content, and an item whose content is a
    // link reference definition alone is empty again after it.
    "- \\n     10) \\r\\n\\r\\n\\t>\\t?><http://a> \"t\"--!> 't'\\t\\t<!-- x</table>\\r|:-:| \"t\" 't'> - <table>| a | b |* M",
    "2) [a]: http://x\\n\\t\\n\\n    :-</style><!--  M",
    "2) [a]: http://x\\n\\n\\n    M",
    "- x\\n  [a]: http://x\\n\\n\\n    M",
    "- [a]: http://x\\n  x\\n\\n\\n    M",
    "- [a]: http://x\\n\\n  x\\n\\n    M",
    "<http://a>\\t<b>> > [x](&lt;\\\\</style><p>````\\t\\t[a]: http://x\\t`</a></pre>\\r\\ufeff]]>\\n* [a]:M",
    "[a]\\n* [a]:M",
];

/// The block a marker line may follow.
const BEFORE: &[&str] = &[
    "",
    "para",
    "# h",
    "text\n===",
    "text\n---",
    "***",
    "```\nx\n```",
    "| a | b |\n|---|---|\n| 1 | 2 |",
    "<details>",
    "<!-- a -->",
    "<!-- a\nb -->",
    "<!-- a --> tail",
    "<pre>x</pre>",
    "> quoted",
    "> quoted\n>",
    "- item",
    "1. item",
    "    code",
    "- > quoted",
    "- ```\n  x\n  ```",
    "-",
    "1.",
    "-   ",
    "<pre>\n</script>",
    "<script>\n</pre>",
    "<style>\n</textarea>",
    "<textarea>\n</style>",
];

/// Each before-block, a line end or a blank line, and a marker line
/// indented 0 to 8 spaces or a tab, inside each container, with LF and
/// CRLF.
fn generated() -> Vec<String> {
    let mut out = Vec::new();
    let indents: Vec<String> = (0..=8)
        .map(|n| " ".repeat(n))
        .chain(["\t", " \t", "\t ", "  \t", "\t\t"].map(str::to_string))
        .collect();
    for before in BEFORE {
        for sep in ["\n", "\n\n"] {
            for indent in &indents {
                let plain = if before.is_empty() {
                    format!("{indent}{M}")
                } else {
                    format!("{before}{sep}{indent}{M}")
                };
                for wrap in ["", "> ", ">\t", "- ", "-\t", "1. ", "1.\t"] {
                    let body = contain(&plain, wrap);
                    out.push(body.clone());
                    out.push(body.replace('\n', "\r\n"));
                }
            }
        }
    }
    out.sort();
    out.dedup();
    out
}

/// `body` inside a container: a quote's marker on each line, or a list
/// item's marker on the first and as many spaces as it is wide on the rest.
fn contain(body: &str, wrap: &str) -> String {
    if wrap.is_empty() {
        return body.to_string();
    }
    let quote = wrap.starts_with('>');
    let width = wrap
        .chars()
        .fold(0, |c, ch| if ch == '\t' { c + 4 - c % 4 } else { c + 1 });
    let rest = if quote {
        wrap.to_string()
    } else {
        " ".repeat(width)
    };
    body.lines()
        .enumerate()
        .map(|(i, l)| match (i, l.is_empty()) {
            (_, true) if !quote => String::new(),
            (0, _) => format!("{wrap}{l}"),
            _ => format!("{rest}{l}"),
        })
        .collect::<Vec<_>>()
        .join("\n")
}

fn corpus_file() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("tests/samples/markers-oracle.tsv")
}

/// A GFM table whose head is the last line of a paragraph, under 0 to 3
/// lines of prose, its delimiter row indented 0, 3 or 4, then a marker
/// line: indented, at the margin, after a blank line or after a row; each
/// at the text's level, in a quote and in a list item, with LF and CRLF.
fn tables() -> Vec<String> {
    let mut out = Vec::new();
    let heads = [
        ("a | b", "-|-"),
        ("| a | b |", "|---|---|"),
        ("x", ":-"),
        ("<", ":-"),
        ("a | b", "-|-|-"),
    ];
    for prose in ["", "p\n", "p\nq\n", "p\nq\nr\n"] {
        for (head, row) in heads {
            for indent in ["", "   ", "    "] {
                for tail in ["    M", "M", "\n    M", "1 | 2\n    M", "| 1 | 2 |\nM"] {
                    let plain = format!("{prose}{head}\n{indent}{row}\n{tail}").replace('M', M);
                    for wrap in ["", "> ", "- ", "1. "] {
                        let body = contain(&plain, wrap);
                        out.push(body.clone());
                        out.push(body.replace('\n', "\r\n"));
                    }
                }
            }
        }
    }
    out
}

/// Every body of the corpus: the chosen and the generated.
fn bodies() -> Vec<String> {
    let mut all: Vec<String> = CHOSEN.iter().map(|b| unescape(b)).collect();
    all.extend(generated());
    all.extend(tables());
    all
}

/// The recorded corpus: each body and the classes comrak gave it.
fn recorded() -> Vec<(String, Vec<Class>)> {
    std::fs::read_to_string(corpus_file())
        .unwrap()
        .lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|l| {
            let (classes, body) = l.split_once('\t').unwrap();
            let classes = classes
                .split(',')
                .filter(|c| !c.is_empty())
                .map(Class::of)
                .collect();
            (unescape(body), classes)
        })
        .collect()
}

#[test]
fn the_recorded_corpus_is_the_oracle_s_answer() {
    if std::env::var_os("TEHAS_RECORD_ORACLE").is_some() {
        let mut file = format!(
            "# Each body's markers as GitHub shows them, by {ORACLE}: live, code,\n# quote, text or none, in order, each asked alone, a tab, then the body\n# with M for {M} and \\\\, \\n, \\r, \\t, \\u00a0 and \\ufeff escaped.\n# Written by TEHAS_RECORD_ORACLE=1 cargo test -p tehas-github --test markers_oracle.\n"
        );
        for body in bodies() {
            let classes: Vec<&str> = classes(&body).iter().map(|c| c.word()).collect();
            file.push_str(&format!("{}\t{}\n", classes.join(","), escape(&body)));
        }
        std::fs::write(corpus_file(), file).unwrap();
    }
    let recorded = recorded();
    let all = bodies();
    assert_eq!(
        recorded.len(),
        all.len(),
        "the corpus file is not the corpus"
    );
    for ((body, recorded), want) in recorded.iter().zip(&all) {
        assert_eq!(body, want);
        assert_eq!(&classes(body), recorded, "{:?}", escape(body));
    }
}

#[test]
fn the_reader_never_reads_a_marker_github_shows_as_code_a_quote_or_words() {
    let mut unsafe_ = Vec::new();
    for (body, classes) in recorded() {
        let read = reader(&body);
        assert_eq!(read.len(), classes.len(), "{:?}", escape(&body));
        for (i, (live, class)) in read.iter().zip(&classes).enumerate() {
            if unsafe_read(*live, *class) {
                unsafe_.push(format!("{:?} marker {i}: {class:?}", escape(&body)));
            }
        }
    }
    assert!(
        unsafe_.is_empty(),
        "{} read where GitHub shows no hidden marker:\n{}",
        unsafe_.len(),
        unsafe_.join("\n")
    );
}

#[test]
fn the_reader_reads_every_marker_github_hides_but_the_known_safe_departures() {
    let mut missed = Vec::new();
    for (body, classes) in recorded() {
        let read = reader(&body);
        let safe = SAFE.iter().any(|(b, _)| unescape(b) == body);
        for (i, (live, class)) in read.iter().zip(&classes).enumerate() {
            if !*live && *class == Class::Live && !safe {
                missed.push(format!("{:?} marker {i}", escape(&body)));
            }
        }
    }
    assert!(
        missed.is_empty(),
        "{} not read where GitHub hides a marker:\n{}",
        missed.len(),
        missed.join("\n")
    );
    // Each safe departure is still one.
    for (b, why) in SAFE {
        let body = unescape(b);
        assert!(
            reader(&body)
                .iter()
                .zip(classes(&body))
                .any(|(l, c)| !l && c == Class::Live),
            "{b:?} ({why}) is no departure now"
        );
    }
}

#[test]
fn every_factory_shape_reads() {
    for b in &CHOSEN[..17] {
        let body = unescape(b);
        let want: Vec<bool> = classes(&body).iter().map(|c| *c == Class::Live).collect();
        assert!(want.iter().all(|l| *l), "{b:?} is no factory shape");
        assert_eq!(reader(&body), want, "{b:?}");
    }
}

/// The fragments the random bodies are built from: markers, containers,
/// tabs and spaces, fences, HTML of each kind, table rows, backticks,
/// backslashes, links and their destinations, headings, breaks and line
/// ends.
const FRAGMENTS: &[&str] = &[
    "M",
    "M",
    "M",
    "> ",
    ">",
    ">>",
    "> > ",
    "- ",
    "* ",
    "+ ",
    "-",
    "1. ",
    "1.",
    "2) ",
    "10) ",
    "99999. ",
    "- - ",
    "> - ",
    "\t",
    "\t",
    "\t\t",
    " ",
    "  ",
    "   ",
    "    ",
    "     ",
    "        ",
    "\u{feff}",
    "```",
    "````",
    "~~~",
    "~~~~",
    "``` x",
    "```rust",
    "~~~ `x`",
    "<div>",
    "</div>",
    "<pre>",
    "</pre>",
    "<script>",
    "</script>",
    "<style>",
    "</style>",
    "<textarea>",
    "</textarea>",
    "<?",
    "?>",
    "<!X",
    "<![CDATA[",
    "]]>",
    "<!-- x",
    "<!--",
    "-->",
    "--!>",
    "<a href=\"x\">",
    "</a>",
    "<b>",
    "<x-y>",
    "</x-y>",
    "<table>",
    "</table>",
    "<p>",
    "| a | b |",
    "|---|---|",
    "| c |",
    "|:-:|",
    "a | b",
    "-|-",
    ":-",
    "p\n",
    "`",
    "``",
    "\\`",
    "\\\\`",
    "\\",
    "\\\\",
    "\\<",
    "&lt;",
    "&#60;",
    "x",
    "text",
    "word ",
    "# ",
    "## ",
    "===",
    "---",
    "***",
    "___",
    "[a]: http://x",
    "[a]:",
    "[x](",
    "](http://x)",
    "](",
    ")",
    " \"t\"",
    " 't'",
    "<http://a>",
    "![i](",
    "\n",
    "\n",
    "\n",
    "\n\n",
    "\r",
    "\r",
    "\r\n",
    "\r\n\r\n",
    "\n\t",
    "\n    ",
    "\n>",
];

/// The random section's seeds, and how many bodies each makes: the same
/// bodies at each run, compared live with comrak and not recorded. A run
/// may ask for other seeds with `TEHAS_ORACLE_SEEDS` (comma separated) and
/// `TEHAS_ORACLE_COUNT`; the gate sets neither.
const SEEDS: &[u64] = &[
    0x7e4a_5c0d_e2b1_9f37,
    1,
    2,
    3,
    0x51ed_2701,
    0x9e37_79b9_7f4a_7c15,
    0xc0ff_ee00_d15e_a5e5,
    0x0123_4567_89ab_cdef,
];
const PER_SEED: usize = 2_500;

/// Bodies of 2 to 25 fragments drawn by a linear congruential generator
/// from `seed`, each with a marker in it.
fn random_bodies(seed: u64, count: usize) -> Vec<String> {
    let mut state = seed;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };
    (0..count)
        .map(|_| {
            let n = 2 + next() % 24;
            let mut body: String = (0..n)
                .map(|_| FRAGMENTS[next() % FRAGMENTS.len()])
                .collect::<String>()
                .replace('M', M);
            if !body.contains(WORDS) {
                body.push_str(M);
            }
            body
        })
        .collect()
}

#[test]
fn the_reader_never_reads_a_marker_github_shows_as_code_a_quote_or_words_in_random_bodies() {
    let seeds: Vec<u64> = match std::env::var("TEHAS_ORACLE_SEEDS") {
        Ok(s) => s.split(',').map(|s| s.trim().parse().unwrap()).collect(),
        Err(_) => SEEDS.to_vec(),
    };
    let count = std::env::var("TEHAS_ORACLE_COUNT").map_or(PER_SEED, |s| s.parse().unwrap());
    let mut unsafe_ = Vec::new();
    let (mut asked, mut dropped) = (0, 0);
    for seed in &seeds {
        for body in random_bodies(*seed, count) {
            for (one, (live, class)) in alone(&body)
                .iter()
                .zip(reader(&body).into_iter().zip(classes(&body)))
            {
                asked += 1;
                dropped += usize::from(class == Class::None);
                if unsafe_read(live, class) {
                    unsafe_.push(format!("seed {seed}: {:?}: {class:?}", escape(one)));
                }
            }
        }
    }
    unsafe_.sort();
    unsafe_.dedup();
    assert!(
        unsafe_.is_empty(),
        "{} of {asked} markers ({dropped} nowhere) in {count} random bodies from each of the seeds {seeds:?} read where GitHub shows no hidden marker:\n{}",
        unsafe_.len(),
        unsafe_.join("\n")
    );
}

#[test]
fn a_marker_inside_a_larger_comment_is_hidden_html_to_both() {
    let body = unescape("<!--\\nM\\n-->");
    assert_eq!(classes(&body), [Class::Live]);
    assert_eq!(reader(&body), [true]);
}

/// A probe for a local run: `TEHAS_ORACLE_PROBE` holds bodies written as
/// the corpus writes them, each on its own line; each marker's reading
/// and class are printed. The gate sets nothing, and it does nothing.
#[test]
fn probe() {
    let Ok(bodies) = std::env::var("TEHAS_ORACLE_PROBE") else {
        return;
    };
    for b in bodies.lines() {
        let body = unescape(b);
        eprintln!(
            "{b:?}\treader {:?}\tcomrak {:?}",
            reader(&body),
            classes(&body)
        );
    }
}
