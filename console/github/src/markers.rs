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
//! Another item's marker is ignored: one whose blueprint field names a
//! blueprint other than the item read. It is no marker of this text, read
//! or `Unreadable`, and its words stay in the text, as 1.x left them. A
//! marker of this item that does not read stays `Unreadable`.
//!
//! What is no marker: one on a quoted line (its first character that is no
//! white space is `>`), inside a fenced code block, or inside a code span
//! of one line. An indented line is text: tehas's writers indent nothing,
//! and a list's continuation line is indented too.
//!
//! [`write`] is the reading's inverse: a text and its markers made into
//! the one text GitHub keeps, each marker's words from `table.rs`, and
//! held to read back as what it was given.

use std::num::{NonZeroU32, NonZeroU64};

use crate::table::{self, MarkerRow, Rule};
use tehas_ports::{
    BlueprintNo, FinishMarker, FinishResult, Got, KeyText, LastAttemptMarker, Marker, MarkerEdit,
    MarkerKind, MarkerWord, MillwrightAction, MillwrightMarker, Outcome, ParkMarker, QuestionKind,
    Ref, Refusal, Sha256, TicketKey,
};

/// A text as the ports carry it: without our markers, and the markers.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Read {
    /// The text without the wrapped markers. A text that held none is the
    /// text as it came. Another item's marker is no marker of it and stays. Where one was taken out, a line left with nothing
    /// but white space goes with it, and so do the blank lines at the
    /// text's start and the white space at its end.
    pub text: String,
    /// The markers, in the order they stand in the text.
    pub markers: Vec<Marker>,
}

/// The markers of `body` and the text without them. `item` is the item the
/// text is read from. A marker, wrapped or bare, whose blueprint field
/// names another blueprint is ignored: it is not in `markers`, neither as
/// read nor as `Unreadable`, and its words stay in `text`. 1.x matched
/// each marker with this issue's number in its pattern and handed the
/// rest of the body on whole: a reply's text is its whole body, and the
/// millwright's reply loses its own first line alone
/// (`console/dispatch/src/questions.rs` lines 174, 178 and 193 at v1.4.0);
/// the millwright's change note is the whole comment
/// (`console/dispatch/src/apply/start.rs` lines 391, 443 and 268 to 278).
/// So another item's marker is never this ticket's, a copy of one holds
/// nothing, and a reader still falls back to an older text.
pub fn read(body: &str, item: Option<BlueprintNo>) -> Read {
    let rows = table::marker_rows();
    let lines = lines(body);
    let dead = dead(body, &lines);
    let mut found = wrapped(body, &lines, &dead, &rows, item);
    let bare = bare(body, &lines, &dead, &found, &rows, item);
    found.extend(bare);
    found.sort_by_key(|f| f.at);
    let spans: Vec<(usize, usize)> = found.iter().filter_map(|f| f.span).collect();
    let mut markers: Vec<Marker> = found.into_iter().filter_map(|f| f.marker).collect();
    twice(&mut markers, &rows);
    Read {
        text: without(body, &lines, &spans),
        markers,
    }
}

/// Whether 1.x put a marker of this kind on a comment's first line, its
/// text after it on the next: the millwright's (its readers take it there,
/// `console/dispatch/src/questions.rs` line 326 and
/// `console/dispatch/src/finish.rs` line 166) and a finish's reply
/// (`console/dispatch/src/finish.rs` line 302, `auto_finish.rs` line 90).
/// Every other kind goes after the text, a blank line between, as
/// `q_render`, the park's comment, the hold's clearing and the last
/// attempt's note put them.
fn leads(kind: &MarkerKind) -> bool {
    matches!(kind, MarkerKind::Millwright | MarkerKind::Finish)
}

/// The text GitHub keeps for `text` and `markers`, written on the item
/// `item`, each marker where 1.x put one of its kind ([`leads`]): a leading
/// marker alone on the first line, the text on the next; the rest after
/// the text and a blank line, each alone on its line in the order given, a
/// block as its open line, its note and its close line. Each marker's
/// words are its row's, as `table::marker_text` writes them. A text with
/// no marker is kept as it is.
///
/// `read(&write(text, markers, item)?, item)` answers that text and
/// `markers` in the order [`laid_out`] gives, or it is refused, so a write that uses it sends nothing.
/// `Refused(Invalid)`: a marker that is not writable; a text or a block's
/// note that holds our wrapper anywhere, in a quote or code too
/// (`tehas_core::text::has_marker`, as 1.x's readers find one: no body
/// carries a cited marker), or a bare marker; a text beside a marker that
/// is not as [`kept`] gives it (the reader takes the blank lines at its
/// start and the white space at its end, and a settle compares the sum of
/// the text read back with the sum of the text sent); and a whole that
/// would not read back as given: a fence the text leaves open, two blocks
/// of one kind, a marker of another item, a note that holds its close
/// line. Nothing is escaped or trimmed. `Refused(Unsupported)`: a
/// project's own marker, whose words the table does not hold, and two
/// markers that would each have to be the first line.
pub fn write(text: &str, markers: &[Marker], item: Option<BlueprintNo>) -> Got<String> {
    let invalid = Outcome::Refused(Refusal::Invalid);
    let marked_note = |m: &Marker| matches!(m, Marker::LastAttempt(l) if foreign_text(&l.note));
    if foreign_text(text) || markers.iter().any(|m| !m.writable() || marked_note(m)) {
        return Err(invalid);
    }
    if markers.is_empty() {
        return Ok(text.to_string());
    }
    if kept(text) != text {
        return Err(invalid);
    }
    let body = compose(text, markers)?;
    let back = read(&body, item);
    if back.text != text || back.markers != laid_out(markers) {
        return Err(invalid);
    }
    Ok(body)
}

/// Whether a text a caller gives may not go into a body: it holds our
/// wrapper anywhere, as `tehas_core::text::has_marker` and 1.x's readers
/// find it, or anything [`read`] reports as a marker.
pub fn foreign_text(text: &str) -> bool {
    tehas_core::text::has_marker(text) || holds_marker(text)
}

/// `markers` in the order [`write`] lays them out, and [`read`] gives them
/// back: a leading one first, the rest in the order given.
pub fn laid_out(markers: &[Marker]) -> Vec<Marker> {
    let (mut lead, rest): (Vec<Marker>, Vec<Marker>) =
        markers.iter().cloned().partition(|m| leads(&m.kind()));
    lead.extend(rest);
    lead
}

/// `text` and `markers` laid out as [`write`] says, unchecked.
fn compose(text: &str, markers: &[Marker]) -> Got<String> {
    let line = |m: &Marker| table::marker_text(m).ok_or(Outcome::Refused(Refusal::Unsupported));
    let (lead, rest): (Vec<&Marker>, Vec<&Marker>) = markers.iter().partition(|m| leads(&m.kind()));
    if lead.len() > 1 {
        return Err(Outcome::Refused(Refusal::Unsupported));
    }
    let rest = rest.into_iter().map(line).collect::<Got<Vec<String>>>()?;
    let mut body = String::new();
    if let Some(m) = lead.first() {
        body.push_str(&line(m)?);
        if !text.is_empty() || !rest.is_empty() {
            body.push('\n');
        }
    }
    body.push_str(text);
    if !rest.is_empty() {
        if !text.is_empty() {
            body.push_str("\n\n");
        }
        body.push_str(&rest.join("\n"));
    }
    Ok(body)
}

/// The body an edit leaves: `body` with its text replaced by `text` when
/// given, and its markers changed as `edit` says, every other byte where it
/// was. A marker set replaces the one of its kind and key in its place, or
/// is added where [`write`] puts its kind; a last attempt's block set goes
/// last, in place of an older one, as 1.x's `last_attempt::decide` put it.
/// One named in `drop` is taken out: its own line, or its words alone where
/// it stands among others, with one space beside it. A marker that does
/// not read, and another item's marker, stay as they are. Adding a marker
/// after the text takes the white space off the body's end, as 1.x did.
/// None when the body would not change.
///
/// `Refused(Invalid)` for a marker set that is not writable, or a text
/// beside a marker that is not as [`kept`] gives it. `Refused(Unsupported)`
/// when the change cannot be made with every other byte kept: a marker to
/// change whose words stand in the body more than once or not as written,
/// a text to replace that does not stand in the body in one piece, or a
/// result that does not read back as asked.
pub fn edit(
    body: &str,
    text: Option<&str>,
    edit: &MarkerEdit,
    item: Option<BlueprintNo>,
) -> Got<Option<String>> {
    let invalid = Outcome::Refused(Refusal::Invalid);
    let unsupported = Outcome::Refused(Refusal::Unsupported);
    if !edit.set.iter().all(Marker::writable) {
        return Err(invalid);
    }
    let before = read(body, item);
    let reads = |m: &Marker| {
        !matches!(
            m,
            Marker::Unreadable { .. } | Marker::Bare { .. } | Marker::Unknown
        )
    };
    let mut out = body.to_string();
    let mut want = before.markers.clone();
    // The text first: the one piece of the body that is the text as read.
    if let Some(t) = text {
        if before.text.is_empty() {
            if !before.markers.iter().all(reads) {
                return Err(unsupported);
            }
            out = compose(t, &before.markers)?;
        } else {
            let at = once(&out, &before.text).ok_or(unsupported.clone())?;
            out.replace_range(at..at + before.text.len(), t);
        }
    }
    for (kind, key) in &edit.drop {
        let gone: Vec<Marker> = want
            .iter()
            .filter(|m| reads(m) && m.kind() == *kind && m.key() == *key)
            .cloned()
            .collect();
        for m in gone {
            take_out(&mut out, &m)?;
            want.retain(|w| *w != m);
        }
    }
    for set in &edit.set {
        let old = want
            .iter()
            .position(|m| reads(m) && m.kind() == set.kind() && m.key() == set.key());
        let words = table::marker_text(set).ok_or(unsupported.clone())?;
        match old {
            Some(at) if set.kind() != MarkerKind::LastAttempt => {
                let from = table::marker_text(&want[at]).ok_or(unsupported.clone())?;
                let i = once(&out, &from).ok_or(unsupported.clone())?;
                out.replace_range(i..i + from.len(), &words);
                want[at] = set.clone();
            }
            old => {
                if let Some(at) = old {
                    take_out(&mut out, &want[at].clone())?;
                    want.remove(at);
                }
                if leads(&set.kind()) {
                    if want.iter().any(|m| leads(&m.kind())) {
                        return Err(unsupported);
                    }
                    out = format!("{words}\n{out}");
                    want.insert(0, set.clone());
                } else {
                    let end = out.trim_end().len();
                    out.truncate(end);
                    let last = out.rsplit('\n').next().unwrap_or_default();
                    let tail = !last.is_empty() && read(last, item).text.trim().is_empty()
                        || table::marker_rows()
                            .iter()
                            .any(|r| r.close.as_deref() == Some(last.trim_end_matches('\r')));
                    out.push_str(match (out.is_empty(), tail) {
                        (true, _) => "",
                        (false, true) => "\n",
                        (false, false) => "\n\n",
                    });
                    out.push_str(&words);
                    want.push(set.clone());
                }
            }
        }
    }
    if out == body {
        return Ok(None);
    }
    let after = read(&out, item);
    let wanted = match text {
        Some(t) if !after.markers.is_empty() && kept(t) != t => return Err(invalid),
        Some(t) => t.to_string(),
        None => before.text.clone(),
    };
    let one = |s: &str| {
        kept(s)
            .split(' ')
            .filter(|w| !w.is_empty())
            .collect::<Vec<_>>()
            .join(" ")
    };
    if after.markers != want || one(&after.text) != one(&wanted) {
        return Err(unsupported);
    }
    Ok(Some(out))
}

/// Where `what` stands in `body`, when it stands there exactly once.
fn once(body: &str, what: &str) -> Option<usize> {
    let mut at = body.match_indices(what).map(|(i, _)| i);
    let first = at.next()?;
    at.next().is_none().then_some(first)
}

/// Take marker `m`'s words out of `body`: its line with its line end when
/// it stands alone on it, else its words and one space beside them.
fn take_out(body: &mut String, m: &Marker) -> Got<()> {
    let unsupported = Outcome::Refused(Refusal::Unsupported);
    let words = table::marker_text(m).ok_or(unsupported.clone())?;
    let s = once(body, &words).ok_or(unsupported)?;
    let e = s + words.len();
    let start = body[..s].rfind('\n').map_or(0, |i| i + 1);
    let end = body[e..].find('\n').map_or(body.len(), |i| e + i);
    let alone = body[start..s].trim().is_empty() && body[e..end].trim().is_empty();
    if !alone {
        let (s, e) = match (body[..s].ends_with(' '), body[e..].starts_with(' ')) {
            (true, true) => (s, e + 1),
            _ => (s, e),
        };
        body.replace_range(s..e, "");
    } else if end < body.len() {
        body.replace_range(start..end + 1, "");
    } else {
        // The last line: its line end before it goes with it.
        let from = body[..start]
            .strip_suffix('\n')
            .map(|b| b.strip_suffix('\r').unwrap_or(b).len())
            .unwrap_or(start);
        body.truncate(from);
    }
    Ok(())
}

/// Whether [`read`] finds any marker in `text`, of any item.
pub fn holds_marker(text: &str) -> bool {
    !read(text, None).markers.is_empty()
}

/// `text` as [`read`] leaves the text around a marker it took out: without
/// the white space at its end and the blank lines at its start. The text
/// [`write`] takes beside a marker.
pub fn kept(text: &str) -> &str {
    let text = text.trim_end();
    let blank = text
        .split_inclusive('\n')
        .take_while(|l| l.trim().is_empty())
        .map(str::len)
        .sum::<usize>();
    &text[blank..]
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
    let mut fence: Option<(u8, usize)> = None;
    for l in lines {
        let text = body[l.start..l.end].trim_start();
        let run = |c: u8| text.bytes().take_while(|b| *b == c).count();
        match fence {
            Some((c, n)) => {
                out.push((l.start, l.next));
                if run(c) >= n && text[run(c)..].trim().is_empty() {
                    fence = None;
                }
            }
            None => match opens(text) {
                Some(open) => {
                    out.push((l.start, l.next));
                    fence = Some(open);
                }
                None if text.starts_with('>') => out.push((l.start, l.next)),
                None => spans(&body[l.start..l.end], l.start, &mut out),
            },
        }
    }
    out
}

/// The fence a line opens, as CommonMark and GitHub read it: three or more
/// backticks or tildes, and a backtick fence's words after it hold no
/// backtick. It closes at a line of the same character, at least as many,
/// with nothing after them.
fn opens(text: &str) -> Option<(u8, usize)> {
    let c = *text
        .as_bytes()
        .first()
        .filter(|c| **c == b'`' || **c == b'~')?;
    let n = text.bytes().take_while(|b| *b == c).count();
    (n >= 3 && (c == b'~' || !text[n..].contains('`'))).then_some((c, n))
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
/// part of the text no bare marker is looked for in, and the marker, None
/// for another item's.
struct Found {
    at: usize,
    span: Option<(usize, usize)>,
    shadow: (usize, usize),
    marker: Option<Marker>,
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

/// Whether the fields after a kind's word name another blueprint than
/// `item`: a `name=value` of the row's blueprint field among the leading
/// `name=value` words, the value a blueprint number. The 1.x word of the
/// millwright goes by the millwright's row.
fn foreign(word: &str, fields: &str, rows: &[MarkerRow], item: Option<BlueprintNo>) -> bool {
    let Some(item) = item else {
        return false;
    };
    let row = row_of(word, rows).or_else(|| {
        word.eq_ignore_ascii_case(table::OLD_MILLWRIGHT)
            .then(|| rows.iter().find(|r| r.kind == MarkerKind::Millwright))
            .flatten()
    });
    let Some(field) = row.and_then(|r| r.fields.iter().find(|f| f.rule == Rule::Blueprint)) else {
        return false;
    };
    fields
        .split_whitespace()
        .take_while(|t| t.contains('='))
        .filter_map(|t| t.strip_prefix(field.name.as_str())?.strip_prefix('='))
        .any(|v| fits(&Rule::Blueprint, v) && v.parse::<u64>().is_ok_and(|n| n != item.get()))
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
        let fields = &rest[table::OURS.len() + word.len()..];
        let fields = fields.split("-->").next().unwrap_or_default();
        let close = after.find("-->").map(|i| at + open.len() + i + 3);
        let fields = match close {
            Some(_) => fields,
            None => fields.lines().next().unwrap_or_default(),
        };
        if foreign(word, fields, rows, item) {
            covered = close.unwrap_or(line.end.max(at + open.len()));
            out.push(Found {
                at,
                span: None,
                shadow: (at, covered),
                marker: None,
            });
            continue;
        }
        let Some(end) = close else {
            // No close: the wrapper is reported, and its words stay.
            covered = line.end.max(at + open.len());
            out.push(Found {
                at,
                span: None,
                shadow: (at, covered),
                marker: Some(unread(word, rows)),
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
                    _ => values.and_then(|v| build(row, &v, note)),
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
            marker: Some(marker),
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
/// number the ports' types do not take. Another item's marker never comes
/// here: [`foreign`] takes it first.
fn build(row: &MarkerRow, values: &[(&str, &str)], note: Option<&str>) -> Option<Marker> {
    let get = |name: &str| values.iter().find(|(n, _)| *n == name).map(|(_, v)| *v);
    let no = |name: &str| BlueprintNo::new(get(name)?.parse().ok()?);
    let marker = match row.kind {
        MarkerKind::Question => Marker::Question {
            kind: match get("kind")? {
                "plan" => QuestionKind::Plan,
                "park" => QuestionKind::Park,
                _ => return None,
            },
            blueprint: no("issue")?,
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
        MarkerKind::Park => {
            let mut park = ParkMarker::new(no("issue")?, Sha256::new(get("sig")?).ok()?);
            park.step = MarkerWord::new(get("step")?).ok()?;
            park.kind = MarkerWord::new(get("kind")?).ok()?;
            park.provider = get("provider")? == "1";
            Marker::Park(park)
        }
        MarkerKind::Finish => {
            let result = match get("result")? {
                "opened" => FinishResult::Opened,
                "refused" => FinishResult::Refused,
                "stopped" => FinishResult::Stopped,
                _ => return None,
            };
            let mut finish = FinishMarker::new(no("issue")?, result);
            finish.by_poll = get("by").is_some();
            Marker::Finish(finish)
        }
        MarkerKind::LastAttempt => {
            let mut last = LastAttemptMarker::new(Sha256::new(get("base")?).ok()?);
            last.note = note?.to_string();
            Marker::LastAttempt(last)
        }
        MarkerKind::Millwright => Marker::Millwright(MillwrightMarker::new(
            no("issue")?,
            match get("action")? {
                "retry" => MillwrightAction::Retry,
                "change" => MillwrightAction::Change,
                "answer" => MillwrightAction::Answer,
                "finish" => MillwrightAction::Finish,
                "person" => MillwrightAction::Person,
                "ask" => MillwrightAction::Ask,
                _ => return None,
            },
        )),
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
            if foreign(&row.word, rest, rows, item) {
                continue;
            }
            let read = match row.close {
                Some(_) => None,
                None => values(row, rest).and_then(|v| build(row, &v, None)),
            };
            out.push(Found {
                at,
                span: None,
                shadow: (at, l.end),
                marker: Some(Marker::Bare {
                    kind: row.kind.clone(),
                    read: read.map(Box::new),
                }),
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
