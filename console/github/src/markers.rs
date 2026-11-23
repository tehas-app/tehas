//! tehas's markers in a text of GitHub's: an issue's body, a comment. The
//! reading takes them out of the text and answers them apart, in the order
//! they stand in it, as the ports spec's "What is a marker" says. The words
//! and the fields of each kind are the rows of `table.rs`; this module
//! writes none of them.
//!
//! Three readings. Wrapped: our word after a `<!--`, and the text to the
//! first `-->` exactly its row's form. Badly wrapped: our word of a kind,
//! and anything else in the wrapper, which is `Unreadable` of that kind and
//! never dropped. Bare: the kind's word and its first field with no
//! wrapper, which stays in the text and is reported as `Bare`.
//!
//! What is no marker: one on a quoted line (its first character that is no
//! white space is `>`), inside a fenced code block, or inside a code span
//! of one line. An indented line is text: tehas's writers indent nothing,
//! and a list's continuation line is indented too.

use std::num::{NonZeroU32, NonZeroU64};

use crate::table::{self, MarkerRow, Rule};
use tehas_ports::{
    BlueprintNo, FinishMarker, FinishResult, KeyText, LastAttemptMarker, Marker, MarkerKind,
    MarkerWord, MillwrightAction, MillwrightMarker, ParkMarker, QuestionKind, Ref, Sha256,
    TicketKey,
};

/// A text as the ports carry it: without our markers, and the markers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    /// The text without the wrapped markers. A text that held none is the
    /// text as it came. Where one was taken out, a line left with nothing
    /// but white space goes with it, and so do the blank lines at the
    /// text's start and the white space at its end.
    pub text: String,
    /// The markers, in the order they stand in the text.
    pub markers: Vec<Marker>,
}

/// The markers of `body` and the text without them. `item` is the item the
/// text is read from: a marker that names another blueprint does not read.
pub fn read(body: &str, item: Option<BlueprintNo>) -> Read {
    let rows = table::marker_rows();
    let lines = lines(body);
    let dead = dead(body, &lines);
    let mut found = wrapped(body, &lines, &dead, &rows, item);
    let bare = bare(body, &lines, &dead, &found, &rows, item);
    found.extend(bare);
    found.sort_by_key(|f| f.at);
    let spans: Vec<(usize, usize)> = found.iter().filter_map(|f| f.span).collect();
    let mut markers: Vec<Marker> = found.into_iter().map(|f| f.marker).collect();
    twice(&mut markers, &rows);
    Read {
        text: without(body, &lines, &spans),
        markers,
    }
}

/// One line: where it starts, where its text ends, and where the next line
/// starts. A `\r` before the line end is no part of its text.
#[derive(Clone, Copy)]
struct Line {
    start: usize,
    end: usize,
    next: usize,
}

fn lines(body: &str) -> Vec<Line> {
    let mut out = Vec::new();
    let mut start = 0;
    while start < body.len() {
        let next = body[start..]
            .find('\n')
            .map_or(body.len(), |i| start + i + 1);
        let text = body[start..next].trim_end_matches('\n');
        let text = text.strip_suffix('\r').unwrap_or(text);
        out.push(Line {
            start,
            end: start + text.len(),
            next,
        });
        start = next;
    }
    out
}

/// The parts of the text that hold no marker: quoted lines, fenced code
/// with its fences, and the code spans of a line.
fn dead(body: &str, lines: &[Line]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut fence: Option<&str> = None;
    for l in lines {
        let text = body[l.start..l.end].trim_start();
        let opens = ["```", "~~~"].into_iter().find(|f| text.starts_with(f));
        match fence {
            Some(f) => {
                out.push((l.start, l.next));
                if text.starts_with(f) {
                    fence = None;
                }
            }
            None if opens.is_some() => {
                out.push((l.start, l.next));
                fence = opens;
            }
            None if text.starts_with('>') => out.push((l.start, l.next)),
            None => spans(&body[l.start..l.end], l.start, &mut out),
        }
    }
    out
}

/// The code spans of one line: a run of backticks to the next run of the
/// same length.
fn spans(line: &str, at: usize, out: &mut Vec<(usize, usize)>) {
    let bytes = line.as_bytes();
    let run = |from: usize| bytes[from..].iter().take_while(|b| **b == b'`').count();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
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

fn within(ranges: &[(usize, usize)], at: usize) -> bool {
    ranges.iter().any(|(s, e)| (*s..*e).contains(&at))
}

/// One marker found: where it stands, what to take out of the text, the
/// part of the text no bare marker is looked for in, and the marker.
struct Found {
    at: usize,
    span: Option<(usize, usize)>,
    shadow: (usize, usize),
    marker: Marker,
}

/// What a wrapper with this word and nothing readable in it is.
fn unread(word: &str, rows: &[MarkerRow]) -> Marker {
    match row_of(word, rows) {
        Some(row) => Marker::Unreadable {
            kind: row.kind.clone(),
        },
        None if word.eq_ignore_ascii_case(table::OLD_MILLWRIGHT) => Marker::Unreadable {
            kind: MarkerKind::Millwright,
        },
        None => Marker::Unknown,
    }
}

fn row_of<'a>(word: &str, rows: &'a [MarkerRow]) -> Option<&'a MarkerRow> {
    rows.iter().find(|r| r.word.eq_ignore_ascii_case(word))
}

/// Every wrapper of ours outside a quote and code, each read by its row.
fn wrapped(
    body: &str,
    lines: &[Line],
    dead: &[(usize, usize)],
    rows: &[MarkerRow],
    item: Option<BlueprintNo>,
) -> Vec<Found> {
    let mut out: Vec<Found> = Vec::new();
    let mut covered = 0;
    for (at, open) in body.match_indices("<!--") {
        if at < covered || within(dead, at) {
            continue;
        }
        let after = &body[at + open.len()..];
        let rest = after.trim_start_matches(char::is_whitespace);
        let ours = rest
            .get(..table::OURS.len())
            .is_some_and(|w| w.eq_ignore_ascii_case(table::OURS));
        if !ours {
            continue;
        }
        let word = &rest[table::OURS.len()..];
        let word = &word[..word.find(char::is_whitespace).unwrap_or(word.len())];
        let word = word.split("-->").next().unwrap_or_default();
        let line = lines
            .iter()
            .copied()
            .find(|l| (l.start..l.next).contains(&at))
            .unwrap_or(Line {
                start: at,
                end: body.len(),
                next: body.len(),
            });
        let Some(end) = after.find("-->").map(|i| at + open.len() + i + 3) else {
            // No close: the wrapper is reported, and its words stay.
            covered = line.end.max(at + open.len());
            out.push(Found {
                at,
                span: None,
                shadow: (at, covered),
                marker: unread(word, rows),
            });
            continue;
        };
        let whole = &body[at..end];
        let mut span = (at, end);
        let marker = match row_of(word, rows) {
            None => unread(word, rows),
            Some(row) => {
                let values = fields_of(row, whole);
                let block = row.close.as_deref().and_then(|close| {
                    let alone = &body[line.start..line.end] == whole;
                    let shut = lines
                        .iter()
                        .find(|l| l.start >= line.next && &body[l.start..l.end] == close)?;
                    alone.then_some(*shut)
                });
                let note = block.map(|shut| {
                    let held = &body[line.next..shut.start];
                    let held = held.strip_suffix('\n').unwrap_or(held);
                    held.strip_suffix('\r').unwrap_or(held)
                });
                let read = match (&row.close, block) {
                    (Some(_), None) => None,
                    _ => values.and_then(|v| build(row, &v, note, item)),
                };
                // A block runs to its close line whether or not it reads.
                if let Some(shut) = block {
                    span = (line.start, shut.next);
                }
                read.unwrap_or(Marker::Unreadable {
                    kind: row.kind.clone(),
                })
            }
        };
        covered = span.1;
        out.push(Found {
            at,
            span: Some(span),
            shadow: span,
            marker,
        });
    }
    out
}

/// The values of a wrapped marker that is exactly its row's form, None for
/// any other text.
fn fields_of<'a>(row: &MarkerRow, whole: &'a str) -> Option<Vec<(&'a str, &'a str)>> {
    let inner = whole
        .strip_prefix(table::WRAP)?
        .strip_prefix(row.word.as_str())?
        .strip_suffix(" -->")?;
    values(row, inner)
}

/// The fields after a kind's word, each one space and `name=value`, in the
/// row's order, an optional one left out or in its place.
fn values<'a>(row: &MarkerRow, inner: &'a str) -> Option<Vec<(&'a str, &'a str)>> {
    let mut tokens = match inner {
        "" => Vec::new(),
        _ => inner.strip_prefix(' ')?.split(' ').collect(),
    }
    .into_iter()
    .peekable();
    let mut out = Vec::new();
    for f in row.fields.iter().filter(|f| f.rule != Rule::Text) {
        let here = tokens
            .peek()
            .and_then(|t| t.split_once('='))
            .filter(|(name, _)| *name == f.name);
        match here {
            Some((name, value)) => {
                if !fits(&f.rule, value) {
                    return None;
                }
                out.push((name, value));
                tokens.next();
            }
            None if f.optional => {}
            None => return None,
        }
    }
    tokens.next().is_none().then_some(out)
}

/// A decimal number with no leading zero, of at most `digits` digits.
fn number(v: &str, digits: usize) -> bool {
    !v.is_empty()
        && v.len() <= digits
        && v.bytes().all(|b| b.is_ascii_digit())
        && (v == "0" || !v.starts_with('0'))
}

fn fits(rule: &Rule, v: &str) -> bool {
    match rule {
        Rule::Blueprint => number(v, 9) && v != "0",
        Rule::Number(digits) => number(v, *digits),
        Rule::Numbers { min, max, digits } => {
            let all: Vec<&str> = v.split(',').collect();
            (*min..=*max).contains(&all.len()) && all.iter().all(|n| number(n, *digits))
        }
        Rule::Words(words) => words.iter().any(|w| w == v),
        Rule::Word(max) => {
            v.len() <= *max
                && v.bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        }
        Rule::Hex(len) => {
            v.len() == *len
                && v.bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }
        Rule::Key(max) => !v.is_empty() && v.len() <= *max && KeyText::new(v).is_ok(),
        Rule::Text => false,
    }
}

/// The marker of a row's values, each already held to its rule. None for a
/// marker that does not read: one of another blueprint than `item`, or a
/// number the ports' types do not take.
fn build(
    row: &MarkerRow,
    values: &[(&str, &str)],
    note: Option<&str>,
    item: Option<BlueprintNo>,
) -> Option<Marker> {
    let get = |name: &str| values.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
    let no = |name: &str| BlueprintNo::new(get(name)?.parse().ok()?);
    let this = |name: &str| no(name).filter(|n| item.is_none_or(|i| i == *n));
    let marker = match row.kind {
        MarkerKind::Question => Marker::Question {
            kind: match get("kind")? {
                "plan" => QuestionKind::Plan,
                "park" => QuestionKind::Park,
                _ => return None,
            },
            blueprint: this("issue")?,
            turn: NonZeroU32::new(get("turn")?.parse().ok()?)?,
        },
        MarkerKind::Blocked => Marker::Blocked {
            by: get("by")?
                .split(',')
                .map(|n| {
                    let n = NonZeroU64::new(n.parse().ok()?)?;
                    Some(Ref::Here(TicketKey::Number(n)))
                })
                .collect::<Option<Vec<_>>>()?,
        },
        MarkerKind::HoldCleared => Marker::HoldCleared,
        MarkerKind::Park => Marker::Park(ParkMarker {
            blueprint: this("issue")?,
            step: MarkerWord::new(get("step")?).ok()?,
            kind: MarkerWord::new(get("kind")?).ok()?,
            provider: get("provider")? == "1",
            sig: Sha256::new(get("sig")?).ok()?,
        }),
        MarkerKind::Finish => Marker::Finish(FinishMarker {
            blueprint: this("issue")?,
            result: match get("result")? {
                "opened" => FinishResult::Opened,
                "refused" => FinishResult::Refused,
                "stopped" => FinishResult::Stopped,
                _ => return None,
            },
            by_poll: get("by").is_some(),
        }),
        MarkerKind::LastAttempt => Marker::LastAttempt(LastAttemptMarker {
            base: Sha256::new(get("base")?).ok()?,
            note: note?.to_string(),
        }),
        MarkerKind::Millwright => Marker::Millwright(MillwrightMarker {
            blueprint: this("issue")?,
            action: match get("action")? {
                "retry" => MillwrightAction::Retry,
                "change" => MillwrightAction::Change,
                "answer" => MillwrightAction::Answer,
                "finish" => MillwrightAction::Finish,
                "person" => MillwrightAction::Person,
                "ask" => MillwrightAction::Ask,
                _ => return None,
            },
        }),
        MarkerKind::TicketOf => {
            let key = get("ticket")?;
            let key = match number(key, 19).then(|| key.parse().ok()).flatten() {
                Some(n) => TicketKey::Number(NonZeroU64::new(n)?),
                None => TicketKey::Key(KeyText::new(key).ok()?),
            };
            Marker::TicketOf(Ref::Here(key))
        }
        MarkerKind::Link => Marker::Link {
            blueprint: no("blueprint")?,
            remove: get("remove").is_some(),
        },
        _ => return None,
    };
    Some(marker.checked()).filter(|m| !matches!(m, Marker::Unreadable { .. }))
}

/// Every bare marker: our word and a kind's, at the start of a line or
/// after white space, one space, the row's first field and `=`, outside a
/// quote, code and every wrapper.
fn bare(
    body: &str,
    lines: &[Line],
    dead: &[(usize, usize)],
    wrapped: &[Found],
    rows: &[MarkerRow],
    item: Option<BlueprintNo>,
) -> Vec<Found> {
    let shadows: Vec<(usize, usize)> = wrapped.iter().map(|f| f.shadow).collect();
    let mut out = Vec::new();
    for l in lines {
        let text = &body[l.start..l.end];
        let low = text.to_ascii_lowercase();
        for (i, ours) in low.match_indices(table::OURS) {
            let at = l.start + i;
            let led = text[..i]
                .chars()
                .next_back()
                .is_none_or(char::is_whitespace);
            if !led || within(dead, at) || within(&shadows, at) {
                continue;
            }
            let after = &text[i + ours.len()..];
            let named = rows.iter().find_map(|row| {
                let first = row.fields.first().filter(|f| f.rule != Rule::Text)?;
                let word = after.get(..row.word.len())?;
                let rest = &after[row.word.len()..];
                let field = rest.strip_prefix(' ')?.strip_prefix(first.name.as_str())?;
                (word.eq_ignore_ascii_case(&row.word) && field.starts_with('='))
                    .then_some((row, rest))
            });
            let Some((row, rest)) = named else {
                continue;
            };
            let read = match row.close {
                Some(_) => None,
                None => values(row, rest).and_then(|v| build(row, &v, None, item)),
            };
            out.push(Found {
                at,
                span: None,
                shadow: (at, l.end),
                marker: Marker::Bare {
                    kind: row.kind.clone(),
                    read: read.map(Box::new),
                },
            });
        }
    }
    out
}

/// Two blocks of one kind in one text are each unreadable.
fn twice(markers: &mut [Marker], rows: &[MarkerRow]) {
    for row in rows.iter().filter(|r| r.close.is_some()) {
        let reads = |m: &Marker| {
            m.kind() == row.kind && !matches!(m, Marker::Unreadable { .. } | Marker::Bare { .. })
        };
        if markers.iter().filter(|m| reads(m)).count() > 1 {
            for m in markers.iter_mut().filter(|m| reads(m)) {
                *m = Marker::Unreadable {
                    kind: row.kind.clone(),
                };
            }
        }
    }
}

/// The text without `spans`, as [`Read::text`] says.
fn without(body: &str, lines: &[Line], spans: &[(usize, usize)]) -> String {
    if spans.is_empty() {
        return body.to_string();
    }
    let mut out = String::with_capacity(body.len());
    for l in lines {
        let cut: Vec<&(usize, usize)> = spans
            .iter()
            .filter(|(s, e)| *s < l.next && *e > l.start)
            .collect();
        if cut.is_empty() {
            out.push_str(&body[l.start..l.next]);
            continue;
        }
        let mut kept = String::new();
        let mut from = l.start;
        for (s, e) in cut {
            let stop = (*s).min(l.end);
            if stop > from {
                kept.push_str(&body[from..stop]);
            }
            from = from.max(*e);
        }
        if from < l.end {
            kept.push_str(&body[from..l.end]);
        }
        if !kept.trim().is_empty() {
            out.push_str(&kept);
            out.push_str(&body[l.end.max(from.min(l.next))..l.next]);
        }
    }
    let text = out.trim_end();
    let blank = text
        .split_inclusive('\n')
        .take_while(|l| l.trim().is_empty())
        .map(str::len)
        .sum::<usize>();
    text[blank..].to_string()
}
