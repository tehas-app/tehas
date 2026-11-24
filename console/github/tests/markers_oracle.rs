//! The marker reader held to a CommonMark and GFM parser, comrak, as the
//! oracle of where GitHub shows a marker. comrak is a dev-dependency of
//! this crate alone; nothing outside its tests uses it.
//!
//! The classes, from comrak's tree, for each marker in a body:
//! - `live`: its words are an HTML block or inline HTML at the text's
//!   level or in a list item, which GitHub keeps as hidden HTML;
//! - `code`: inside a code block or a code span;
//! - `quote`: inside a block quote, at any depth;
//! - `text`: shown as words (comrak read no HTML there).
//!
//! The reader must never read a marker that is not `live` (the unsafe
//! way: no difference allowed). It must read every `live` marker, except
//! in the known safe departures, listed by body in [`SAFE`].
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
];

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Class {
    Live,
    Code,
    Quote,
    Text,
}

impl Class {
    fn word(self) -> &'static str {
        match self {
            Class::Live => "live",
            Class::Code => "code",
            Class::Quote => "quote",
            Class::Text => "text",
        }
    }

    fn of(word: &str) -> Class {
        match word {
            "live" => Class::Live,
            "code" => Class::Code,
            "quote" => Class::Quote,
            "text" => Class::Text,
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

/// Whether the reader reads each marker of `body`, in their order: each
/// read alone, the others made no marker of the same shape.
fn reader(body: &str) -> Vec<bool> {
    let at: Vec<usize> = body.match_indices(WORDS).map(|(i, _)| i).collect();
    at.iter()
        .map(|i| {
            let mut one = body.replace(WORDS, OTHER);
            one.replace_range(*i..i + WORDS.len(), WORDS);
            read(&one, BlueprintNo::new(7))
                .markers
                .iter()
                .any(|m| m.kind() == MarkerKind::Finish && matches!(m, Marker::Finish(_)))
        })
        .collect()
}

/// A body as the corpus file writes it: `M` the marker, and `\n`, `\r`,
/// `\t` and ` ` escaped.
fn unescape(s: &str) -> String {
    s.replace("\\u00a0", "\u{a0}")
        .replace("\\ufeff", "\u{feff}")
        .replace("\\r", "\r")
        .replace("\\n", "\n")
        .replace("\\t", "\t")
        .replace('M', M)
}

fn escape(s: &str) -> String {
    s.replace(M, "M")
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
    "\\\\`M\\\\`",
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

/// Every body of the corpus: the chosen and the generated.
fn bodies() -> Vec<String> {
    let mut all: Vec<String> = CHOSEN.iter().map(|b| unescape(b)).collect();
    all.extend(generated());
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
            "# Each body's markers as GitHub shows them, by {ORACLE}: live, code,\n# quote or text, in order, a tab, then the body with M for\n# {M} and \\n, \\r, \\t and \\u00a0 escaped.\n# Written by TEHAS_RECORD_ORACLE=1 cargo test -p tehas-github --test markers_oracle.\n"
        );
        for body in bodies() {
            let classes: Vec<&str> = oracle(&body).iter().map(|c| c.word()).collect();
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
    for ((body, classes), want) in recorded.iter().zip(&all) {
        assert_eq!(body, want);
        assert_eq!(&oracle(body), classes, "{:?}", escape(body));
    }
}

#[test]
fn the_reader_never_reads_a_marker_github_shows_in_code_or_a_quote() {
    let mut unsafe_ = Vec::new();
    for (body, classes) in recorded() {
        let read = reader(&body);
        assert_eq!(read.len(), classes.len(), "{:?}", escape(&body));
        for (i, (live, class)) in read.iter().zip(&classes).enumerate() {
            if *live && *class != Class::Live {
                unsafe_.push(format!("{:?} marker {i}: {class:?}", escape(&body)));
            }
        }
    }
    assert!(
        unsafe_.is_empty(),
        "{} read where GitHub shows no marker:\n{}",
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
                .zip(oracle(&body))
                .any(|(l, c)| !l && c == Class::Live),
            "{b:?} ({why}) is no departure now"
        );
    }
}

#[test]
fn every_factory_shape_reads() {
    for b in &CHOSEN[..17] {
        let body = unescape(b);
        let want: Vec<bool> = oracle(&body).iter().map(|c| *c == Class::Live).collect();
        assert!(want.iter().all(|l| *l), "{b:?} is no factory shape");
        assert_eq!(reader(&body), want, "{b:?}");
    }
}

/// The fragments the random bodies are built from: markers, containers,
/// tabs and spaces, fences, HTML, table rows, backticks and line ends.
const FRAGMENTS: &[&str] = &[
    "M",
    "M",
    "> ",
    ">",
    "- ",
    "-",
    "1. ",
    "1.",
    "10) ",
    "\t",
    "\t",
    " ",
    "  ",
    "   ",
    "    ",
    "        ",
    "```",
    "~~~",
    "<div>",
    "</div>",
    "<pre>",
    "</pre>",
    "| a | b |",
    "|---|---|",
    "`",
    "``",
    "\\`",
    "x",
    "text",
    "# ",
    "===",
    "---",
    "<!--",
    "-->",
    "\n",
    "\n",
    "\n",
    "\n\n",
    "\r",
    "\r\n",
];

/// The random section's seed and size: the bodies are made anew at each
/// run, the same each time, and not recorded.
const SEED: u64 = 0x7e4a_5c0d_e2b1_9f37;
const RANDOM: usize = 20_000;

/// A body of 2 to 13 fragments drawn by a linear congruential generator,
/// with a marker in it.
fn random_bodies() -> Vec<String> {
    let mut state = SEED;
    let mut next = move || {
        state = state
            .wrapping_mul(6_364_136_223_846_793_005)
            .wrapping_add(1_442_695_040_888_963_407);
        (state >> 33) as usize
    };
    (0..RANDOM)
        .map(|_| {
            let n = 2 + next() % 12;
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
fn the_reader_never_reads_a_marker_github_shows_in_code_or_a_quote_in_random_bodies() {
    let mut unsafe_ = Vec::new();
    for body in random_bodies() {
        let classes = oracle(&body);
        let read = reader(&body);
        if read.len() != classes.len() {
            // A table drops a row's cells past its head's, a marker in them
            // with them: GitHub shows it nowhere. The places are then
            // unknown, so the body is skipped; any other difference fails.
            let dropped = read.len() > classes.len() && body.contains("|-");
            if !dropped {
                unsafe_.push(format!(
                    "{:?}: {} markers, comrak {}",
                    escape(&body),
                    read.len(),
                    classes.len()
                ));
            }
            continue;
        }
        for (i, (live, class)) in read.iter().zip(&classes).enumerate() {
            if *live && *class != Class::Live {
                unsafe_.push(format!("{:?} marker {i}: {class:?}", escape(&body)));
            }
        }
    }
    assert!(
        unsafe_.is_empty(),
        "{} of {RANDOM} random bodies (seed {SEED:#x}) read where GitHub shows no marker:\n{}",
        unsafe_.len(),
        unsafe_.join("\n")
    );
}

#[test]
fn a_marker_inside_a_larger_comment_is_hidden_html_to_both() {
    let body = unescape("<!--\\nM\\n-->");
    assert_eq!(oracle(&body), [Class::Live]);
    assert_eq!(reader(&body), [true]);
}
