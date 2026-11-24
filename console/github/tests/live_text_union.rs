//! `live_text` blanks at least what it blanked before the reader parsed
//! blocks: the blanked part only grows, so a closing line is only ever
//! added, never taken for said where it was not before. The older
//! `live_text` (at 196e14d28) is kept here as the reference.

use tehas_github::markers::live_text;

/// The older `live_text`, whole: lines split at LF; a fence's lines, a
/// line starting `>` past spaces and a code span paired on one line; every
/// indented line at the text's start or after a blank line; every HTML
/// comment.
fn reference(body: &str) -> String {
    let mut lines = Vec::new();
    let mut start = 0;
    while start < body.len() {
        let next = body[start..]
            .find('\n')
            .map_or(body.len(), |i| start + i + 1);
        let text = body[start..next].trim_end_matches('\n');
        let text = text.strip_suffix('\r').unwrap_or(text);
        lines.push((start, start + text.len(), next));
        start = next;
    }
    let mut gone = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for &(s, e, n) in &lines {
        let raw = &body[s..e];
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        let text = (indent <= 3).then(|| &raw[indent..]);
        match fence {
            Some((c, len)) => {
                gone.push((s, n));
                let closes = text.is_some_and(|t| {
                    let run = t.bytes().take_while(|b| *b == c).count();
                    run >= len && t[run..].bytes().all(|b| b == b' ' || b == b'\t')
                });
                if closes {
                    fence = None;
                }
            }
            None => match text.and_then(opens) {
                Some(open) => {
                    gone.push((s, n));
                    fence = Some(open);
                }
                None if raw.trim_start().starts_with('>') => gone.push((s, n)),
                None => spans(raw, s, &mut gone),
            },
        }
    }
    let mut code_may_open = true;
    for &(s, e, n) in &lines {
        let raw = &body[s..e];
        if raw.trim().is_empty() {
            code_may_open = true;
            continue;
        }
        if (raw.starts_with("    ") || raw.starts_with('\t')) && code_may_open {
            gone.push((s, n));
        } else {
            code_may_open = false;
        }
    }
    let mut from = 0;
    while let Some(open) = body[from..].find("<!--") {
        let start = from + open;
        let end = body[start + 4..]
            .find("-->")
            .map_or(body.len(), |e| start + 4 + e + 3);
        gone.push((start, end));
        from = end;
    }
    body.char_indices()
        .map(|(i, c)| {
            let blank = gone.iter().any(|(s, e)| (*s..*e).contains(&i));
            if c != '\n' && blank {
                ' '
            } else {
                c
            }
        })
        .collect()
}

fn opens(text: &str) -> Option<(u8, usize)> {
    let c = *text
        .as_bytes()
        .first()
        .filter(|c| **c == b'`' || **c == b'~')?;
    let n = text.bytes().take_while(|b| *b == c).count();
    (n >= 3 && (c == b'~' || !text[n..].contains('`'))).then_some((c, n))
}

fn spans(line: &str, at: usize, out: &mut Vec<(usize, usize)>) {
    let bytes = line.as_bytes();
    let run = |from: usize| bytes[from..].iter().take_while(|b| **b == b'`').count();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let slashes = bytes[..i].iter().rev().take_while(|b| **b == b'\\').count();
        if slashes % 2 == 1 {
            i += 1;
            continue;
        }
        let n = run(i);
        let mut j = i + n;
        let mut close = None;
        while j < bytes.len() {
            if bytes[j] == b'`' {
                let m = run(j);
                if m == n {
                    close = Some(j + m);
                    break;
                }
                j += m;
            } else {
                j += 1;
            }
        }
        match close {
            Some(end) => {
                out.push((at + i, at + end));
                i = end;
            }
            None => i += n,
        }
    }
}

/// The characters the reference blanks and `live_text` leaves, by index.
fn lost(body: &str) -> Vec<usize> {
    let (old, new) = (reference(body), live_text(body));
    body.chars()
        .zip(old.chars().zip(new.chars()))
        .enumerate()
        .filter(|(_, (c, (o, n)))| o != c && n == c)
        .map(|(i, _)| i)
        .collect()
}

/// Closing lines, markdown and HTML, as the reviewer's generator draws them.
const FRAGMENTS: &[&str] = &[
    "Fixes #7.",
    "Fixes #7",
    "> ",
    ">",
    "- ",
    "* ",
    "1. ",
    "\t",
    "    ",
    "  ",
    "```",
    "~~~",
    "<pre>",
    "</pre>",
    "<div>",
    "</div>",
    "<!--",
    "-->",
    "`",
    "``",
    "\\`",
    "| a | b |",
    "|---|---|",
    "a|b",
    "-|-",
    "p",
    "x",
    "[x](",
    "<http://a>",
    "\n",
    "\n",
    "\n",
    "\n\n",
    "\r\n",
    "\r",
    "# ",
    "---",
    "<script>",
    "</script>",
    "[l]: ",
    "\"t\"",
    "\u{feff}",
    "\\",
    "](",
    ")",
];

const SEEDS: &[u64] = &[0x9e37_79b9_7f4a_7c15, 1, 2, 3];
const PER_SEED: usize = 25_000;

#[test]
fn live_text_blanks_all_the_older_one_blanked() {
    let mut losses = Vec::new();
    for seed in SEEDS {
        let mut state = *seed;
        let mut next = move || {
            state ^= state << 13;
            state ^= state >> 7;
            state ^= state << 17;
            state as usize
        };
        for _ in 0..PER_SEED {
            let k = 2 + next() % 20;
            let body: String = (0..k)
                .map(|_| FRAGMENTS[next() % FRAGMENTS.len()])
                .collect();
            if !lost(&body).is_empty() {
                losses.push(format!("{body:?}"));
            }
        }
    }
    assert!(
        losses.is_empty(),
        "{} bodies lose blanked text:\n{}",
        losses.len(),
        losses[..losses.len().min(20)].join("\n")
    );
}

#[test]
fn a_quote_mark_or_a_code_span_inside_an_html_block_stays_blanked() {
    for body in [
        "</pre>\n> |---|---|",
        "<pre>\r\n>```p\nFixes #7.",
        "<div>\n> Fixes #7.",
        "<pre>\nx `Fixes #7.` y",
    ] {
        assert!(lost(body).is_empty(), "{body:?}");
    }
    // The closing line inside an HTML block after a `>` is no closing line
    // said, as before.
    let live = live_text("<div>\n> Fixes #7.");
    assert!(!live.contains("Fixes #7."), "{live:?}");
}
