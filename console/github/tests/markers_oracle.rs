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
const SAFE: &[(&str, &str)] = &[];

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
        NodeValue::CodeBlock(c) => (Some(c.literal.clone()), Class::Code),
        NodeValue::Code(c) => (Some(c.literal.clone()), Class::Code),
        NodeValue::HtmlBlock(h) => (Some(h.literal.clone()), Class::Live),
        NodeValue::HtmlInline(t) => (Some(t.clone()), Class::Live),
        NodeValue::Text(t) => (Some(t.clone()), Class::Text),
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
];

/// Each before-block, a line end or a blank line, and a marker line
/// indented 0 to 8 spaces or a tab, inside each container, with LF and
/// CRLF.
fn generated() -> Vec<String> {
    let mut out = Vec::new();
    let indents: Vec<String> = (0..=8)
        .map(|n| " ".repeat(n))
        .chain(["\t".to_string()])
        .collect();
    for before in BEFORE {
        for sep in ["\n", "\n\n"] {
            for indent in &indents {
                let plain = if before.is_empty() {
                    format!("{indent}{M}")
                } else {
                    format!("{before}{sep}{indent}{M}")
                };
                for wrap in ["", "> ", "- ", "1. "] {
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

/// `body` inside a container: a quote's `> ` on each line, or a list
/// item's marker on the first and its indent on the rest.
fn contain(body: &str, wrap: &str) -> String {
    if wrap.is_empty() {
        return body.to_string();
    }
    let rest = if wrap == "> " {
        "> ".to_string()
    } else {
        " ".repeat(wrap.len())
    };
    body.lines()
        .enumerate()
        .map(|(i, l)| match (i, l.is_empty()) {
            (_, true) if wrap != "> " => String::new(),
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
