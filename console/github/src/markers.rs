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
//! What is no marker, as GitHub renders it as text: one on a quoted line
//! (its first character that is no white space is `>`), or on a line that
//! lazily continues a quote's paragraph (a line right after a quoted line,
//! no blank line between, that starts no block of its own: not a `<!--`,
//! a fence, a heading, a thematic break or a list item); one inside a
//! fenced code block, or an indented code block (a line indented 4 columns
//! or more, a tab to the next 4, after a blank line, the text's start, a
//! fence's close, a heading, a thematic break or a line that is one HTML
//! comment, and outside a list, with the lines indented as much or blank
//! after it); and one inside a code span of one line.
//!
//! What stays live: a line indented 4 or more right after a paragraph's
//! line continues that paragraph, as CommonMark has it, so its marker
//! reads. Inside a list, an indented line is the item's text, as before:
//! a list's continuation line is indented too. A code span across line
//! ends, which CommonMark allows, is read as no span: a known departure. A
//! factory marker stands alone on its line, after a blank line, so none of
//! these reach one: they drop what a person pasted.
//!
//! [`write`] is the reading's inverse: a text and its markers made into
//! the one text GitHub keeps, each marker's words from `table.rs`, and
//! held to read back as what it was given.

use std::num::{NonZeroU32, NonZeroU64};

use crate::table::{self, MarkerRow, Rule};
use tehas_ports::{
    BlueprintNo, CommentId, FinishMarker, FinishResult, Got, KeyText, LastAttemptMarker, Marker,
    MarkerEdit, MarkerKind, MarkerWord, MillwrightAction, MillwrightMarker, Outcome, ParkMarker,
    QuestionKind, Ref, Refusal, Sha256, TicketKey,
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
    located(body, item).0
}

/// [`read`], and beside each marker read the bytes of the body it stands
/// on, None for one whose words stay in the text (bare, or a wrapper with
/// no close).
fn located(body: &str, item: Option<BlueprintNo>) -> (Read, Vec<Option<(usize, usize)>>) {
    let rows = table::marker_rows();
    let lines = lines(body);
    let dead = dead(body, &lines);
    let mut found = wrapped(body, &lines, &dead, &rows, item);
    let bare = bare(body, &lines, &dead, &found, &rows, item);
    found.extend(bare);
    found.sort_by_key(|f| f.at);
    let spans: Vec<(usize, usize)> = found.iter().filter_map(|f| f.span).collect();
    let (mut markers, at): (Vec<Marker>, Vec<Option<(usize, usize)>>) = found
        .into_iter()
        .filter_map(|f| Some((f.marker?, f.span)))
        .unzip();
    twice(&mut markers, &rows);
    let read = Read {
        text: without(body, &lines, &spans),
        markers,
    };
    (read, at)
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
    if foreign_text(text) || !markers.iter().all(Marker::writable) {
        return Err(invalid);
    }
    for m in markers {
        render(m)?;
    }
    if markers.is_empty() {
        return Ok(text.to_string());
    }
    if kept(text) != text {
        return Err(invalid);
    }
    let body = compose(text, markers)?;
    let back = read(&body, item);
    if back.text != text || back.markers != laid_out(markers) || loose(&body, item) > 0 {
        return Err(invalid);
    }
    Ok(body)
}

/// How many of our wrappers, opening or closing, stand in `body` outside
/// the markers that read: what 1.x's readers, which look in quotes, code
/// and a block's note too, would find beside the typed markers. A backstop
/// over the typed words: the caller's own text is refused whole by
/// [`foreign_text`] first, in [`write`] and in [`edit`].
fn loose(body: &str, item: Option<BlueprintNo>) -> usize {
    let (read, at) = located(body, item);
    let typed: Vec<(usize, usize)> = read
        .markers
        .iter()
        .zip(at)
        .filter(|(m, _)| reads(m))
        .filter_map(|(_, s)| s)
        .collect();
    let notes: Vec<usize> = read
        .markers
        .iter()
        .filter_map(|m| match m {
            Marker::LastAttempt(l) => Some(wrappers(&l.note).count()),
            _ => None,
        })
        .collect();
    let outside = wrappers(body)
        .filter(|i| !typed.iter().any(|(s, e)| (*s..*e).contains(i)))
        .count();
    outside + notes.iter().sum::<usize>()
}

/// Where each `<!--` stands that white space and then `tehas:` or
/// `/tehas:`, in any case, follow: our wrapper, opening or closing, as
/// `tehas_core::text::has_marker` finds the opening one.
fn wrappers(text: &str) -> impl Iterator<Item = usize> + '_ {
    text.match_indices("<!--").filter_map(move |(at, _)| {
        let rest = text[at + 4..].trim_start_matches(char::is_whitespace);
        let rest = rest.strip_prefix('/').unwrap_or(rest);
        rest.get(..table::OURS.len())
            .is_some_and(|w| w.eq_ignore_ascii_case(table::OURS))
            .then_some(at)
    })
}

/// A marker that reads: not unreadable, bare or unknown.
fn reads(m: &Marker) -> bool {
    !matches!(
        m,
        Marker::Unreadable { .. } | Marker::Bare { .. } | Marker::Unknown
    )
}

/// A marker's words as its row writes them. `Refused(Invalid)` for a block
/// whose note holds anything [`foreign_text`] refuses;
/// `Refused(Unsupported)` for a marker the table has no words for.
fn render(m: &Marker) -> Got<String> {
    if let Marker::LastAttempt(l) = m {
        if foreign_text(&l.note) {
            return Err(Outcome::Refused(Refusal::Invalid));
        }
    }
    table::marker_text(m).ok_or(Outcome::Refused(Refusal::Unsupported))
}

/// Whether a text a caller gives may not go into a body: it holds our
/// wrapper anywhere, opening or closing (a block's close words end 1.x's
/// last attempt early, `console/cycle/src/last_attempt.rs` line 46), as
/// `tehas_core::text::has_marker` and 1.x's readers find it, or anything
/// [`read`] reports as a marker.
pub fn foreign_text(text: &str) -> bool {
    tehas_core::text::has_marker(text) || wrappers(text).next().is_some() || holds_marker(text)
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
    let line = render;
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
/// was, its CRs among them. Each marker to change is found where the reader
/// finds it in the stored bytes. A marker set replaces the one of its kind
/// and key in its place, or is added where [`write`] puts its kind; one
/// equal to the marker there changes nothing; a last attempt's block set
/// goes last, in place of an older one, as 1.x's `last_attempt::decide` put
/// it. One named in `drop` is taken out: its own line, or its words and the
/// one space beside them where it shares a line. A marker that does not
/// read, another item's marker and a kept text stay as they are. Besides
/// what is asked, an edit may take the white space off the body's end when
/// it adds a marker after the text (as 1.x did), and a blank line next to a
/// line it takes out. An empty text leaves the markers alone. None when the
/// body would not change.
///
/// `Refused(Invalid)`: a text that [`foreign_text`] refuses, a marker set
/// that is not writable or whose block's note it refuses, a text beside a marker that is not as
/// [`kept`] gives it, and a result that holds more of our wrappers outside
/// its typed markers than the body did. `Refused(Unsupported)`: a text to
/// replace that does not stand in the body in one piece, a second marker
/// that would have to be the first line, or a result that does not read
/// back as asked.
pub fn edit(
    body: &str,
    text: Option<&str>,
    edit: &MarkerEdit,
    item: Option<BlueprintNo>,
) -> Got<Option<String>> {
    let invalid = Outcome::Refused(Refusal::Invalid);
    let unsupported = Outcome::Refused(Refusal::Unsupported);
    if !edit.set.iter().all(Marker::writable) || text.is_some_and(foreign_text) {
        return Err(invalid);
    }
    let words = edit.set.iter().map(render).collect::<Got<Vec<String>>>()?;
    let (before, at) = located(body, item);
    let mut slots: Vec<Slot> = before
        .markers
        .iter()
        .cloned()
        .zip(at)
        .map(|(m, s)| (m, s, true))
        .collect();
    // Changes to the stored bytes: a range and what takes its place.
    let mut cuts: Vec<(usize, usize, String)> = Vec::new();
    let mut lead: Option<(Marker, String)> = None;
    let mut tail: Vec<(Marker, String)> = Vec::new();
    match text {
        Some(t) if before.text.is_empty() => {
            if !t.is_empty() {
                let first = slots
                    .first()
                    .filter(|(m, s, _)| leads(&m.kind()) && s.is_some_and(|(s, _)| s == 0));
                let at = match first {
                    Some((_, Some((_, e)), _)) => {
                        body[*e..].find('\n').map_or(body.len(), |i| e + i + 1)
                    }
                    _ => 0,
                };
                let insert = match (at, at == body.len()) {
                    (_, true) if !body.ends_with('\n') && !body.is_empty() => format!("\n{t}"),
                    (_, true) => t.to_string(),
                    _ => format!("{t}\n\n"),
                };
                cuts.push((at, at, insert));
            }
        }
        Some(t) => {
            let s = once(body, &before.text).ok_or(unsupported.clone())?;
            cuts.push((s, s + before.text.len(), t.to_string()));
        }
        None => {}
    }
    for (kind, key) in &edit.drop {
        for slot in slots.iter_mut() {
            let (m, span, live) = slot;
            if *live && reads(m) && m.kind() == *kind && m.key() == *key {
                cuts.push(take_out(body, span.ok_or(unsupported.clone())?));
                *live = false;
            }
        }
    }
    for (set, words) in edit.set.iter().zip(words) {
        let old = slots.iter_mut().find(|(m, _, live)| {
            *live && reads(m) && m.kind() == set.kind() && m.key() == set.key()
        });
        match old {
            Some((m, _, _)) if m == set => {}
            Some((m, Some((s, e)), _)) if set.kind() != MarkerKind::LastAttempt => {
                cuts.push((*s, *e, words));
                *m = set.clone();
            }
            Some((_, None, _)) => return Err(unsupported),
            old => {
                if let Some((_, span, live)) = old {
                    cuts.push(take_out(body, span.ok_or(unsupported.clone())?));
                    *live = false;
                }
                if leads(&set.kind()) {
                    let led = slots.iter().any(|(m, _, live)| *live && leads(&m.kind()));
                    if led || lead.is_some() {
                        return Err(unsupported);
                    }
                    lead = Some((set.clone(), words));
                } else {
                    tail.push((set.clone(), words));
                }
            }
        }
    }
    // The cuts from the end of the body back, none over another.
    cuts.sort_by_key(|c| std::cmp::Reverse(c.0));
    if cuts.windows(2).any(|w| w[1].1 > w[0].0) {
        return Err(unsupported);
    }
    let mut out = body.to_string();
    for (s, e, with) in &cuts {
        out.replace_range(*s..*e, with);
    }
    if text == Some("") {
        out = kept(&out).to_string();
    }
    if let Some((_, words)) = &lead {
        out = match out.is_empty() {
            true => words.clone(),
            false => format!("{words}\n{out}"),
        };
    }
    if !tail.is_empty() {
        let end = out.trim_end().len();
        out.truncate(end);
        let last = out.rsplit('\n').next().unwrap_or_default();
        let after_marker = !last.is_empty() && read(last, item).text.trim().is_empty()
            || table::marker_rows()
                .iter()
                .any(|r| r.close.as_deref() == Some(last.trim_end_matches('\r')));
        out.push_str(match (out.is_empty(), after_marker) {
            (true, _) => "",
            (false, true) => "\n",
            (false, false) => "\n\n",
        });
        let lines: Vec<&str> = tail.iter().map(|(_, w)| w.as_str()).collect();
        out.push_str(&lines.join("\n"));
    }
    if out == body {
        return Ok(None);
    }
    let want: Vec<Marker> = lead
        .iter()
        .map(|(m, _)| m.clone())
        .chain(slots.into_iter().filter(|s| s.2).map(|s| s.0))
        .chain(tail.into_iter().map(|(m, _)| m))
        .collect();
    let after = read(&out, item);
    let wanted = match text {
        Some(t) if !after.markers.is_empty() && kept(t) != t => return Err(invalid),
        Some(t) => t.to_string(),
        None => before.text.clone(),
    };
    // The text as read, its words compared: where a marker shared a line,
    // the space beside it went with it.
    let one = |s: &str| {
        kept(s)
            .lines()
            .map(|l| {
                l.split([' ', '\t'])
                    .filter(|w| !w.is_empty())
                    .collect::<Vec<_>>()
                    .join(" ")
            })
            .collect::<Vec<_>>()
            .join("\n")
    };
    if after.markers != want || one(&after.text) != one(&wanted) {
        return Err(unsupported);
    }
    if loose(&out, item) > loose(body, item) {
        return Err(invalid);
    }
    Ok(Some(out))
}

/// A marker of a body being edited: the marker, its bytes, and whether it
/// is still there.
type Slot = (Marker, Option<(usize, usize)>, bool);

/// Where `what` stands in `body`, when it stands there exactly once.
fn once(body: &str, what: &str) -> Option<usize> {
    let mut at = body.match_indices(what).map(|(i, _)| i);
    let first = at.next()?;
    at.next().is_none().then_some(first)
}

/// The bytes to take out of `body` with the marker at `span`: its line,
/// with its line end, when it stands alone on it, and one blank line more
/// where taking it out would leave two together; else its words and the
/// one space beside them.
fn take_out(body: &str, (s, e): (usize, usize)) -> (usize, usize, String) {
    let core = body[..e].strip_suffix('\n').unwrap_or(&body[..e]);
    let core = core.strip_suffix('\r').unwrap_or(core).len().max(s);
    let start = body[..s].rfind('\n').map_or(0, |i| i + 1);
    let end = body[core..].find('\n').map_or(body.len(), |i| core + i);
    let alone = body[start..s].trim().is_empty() && body[core..end].trim().is_empty();
    if !alone {
        let space = |b: u8| b == b' ' || b == b'\t';
        let before = s > 0 && space(body.as_bytes()[s - 1]);
        let after = body.as_bytes().get(core).copied().is_some_and(space);
        return match (before, after) {
            (_, true) => (s, core + 1, String::new()),
            (true, false) => (s - 1, core, String::new()),
            (false, false) => (s, core, String::new()),
        };
    }
    if end == body.len() {
        // The last line: the line end before it goes with it.
        let head = &body[..start];
        let head = head.strip_suffix('\n').unwrap_or(head);
        let head = head.strip_suffix('\r').unwrap_or(head);
        return (head.len(), end, String::new());
    }
    let mut end = end + 1;
    let blank_before =
        start == 0 || body[..start].ends_with("\n\n") || body[..start].ends_with("\n\r\n");
    let next = &body[end..];
    if blank_before {
        if next.starts_with("\r\n") {
            end += 2;
        } else if next.starts_with('\n') {
            end += 1;
        }
    }
    (start, end, String::new())
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

/// The parts of the text that hold no marker, as the module's doc says:
/// quoted lines and their lazy continuations, fenced and indented code with
/// their fences, and the code spans of a line.
fn dead(body: &str, lines: &[Line]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    // The line before ends a block, so an indented line starts code.
    let mut after_block = true;
    let mut prev_blank = true;
    let mut quote_para = false;
    let mut code = false;
    let mut list = false;
    for l in lines {
        let raw = &body[l.start..l.end];
        let blank = raw.trim().is_empty();
        // A fence's line is indented 0 to 3 spaces; more is code inside
        // or a line of text.
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        let text = (indent <= 3).then(|| &raw[indent..]);
        let deep = columns(raw) >= 4;
        if let Some((c, n)) = fence {
            out.push((l.start, l.next));
            let closes = text.is_some_and(|t| {
                let run = t.bytes().take_while(|b| *b == c).count();
                run >= n && t[run..].bytes().all(|b| b == b' ' || b == b'\t')
            });
            if closes {
                fence = None;
            }
            (after_block, prev_blank, quote_para) = (closes, false, false);
            continue;
        }
        if code && (blank || deep) {
            out.push((l.start, l.next));
            prev_blank = blank;
            continue;
        }
        code = false;
        if blank {
            (after_block, prev_blank, quote_para) = (true, true, false);
            continue;
        }
        if list && prev_blank && !deep && columns(raw) < 2 && !item(raw.trim_start()) {
            list = false;
        }
        if item(raw.trim_start()) && !deep {
            list = true;
        }
        let opened = text.and_then(opens);
        if deep && after_block && !list {
            out.push((l.start, l.next));
            code = true;
        } else if let Some(open) = opened {
            out.push((l.start, l.next));
            fence = Some(open);
        } else if raw.trim_start().starts_with('>') {
            out.push((l.start, l.next));
            quote_para = !raw.trim_start()[1..].trim().is_empty();
        } else if quote_para && !starts_block(raw) {
            out.push((l.start, l.next));
        } else {
            quote_para = false;
            spans(raw, l.start, &mut out);
        }
        let t = raw.trim();
        after_block = heading(t) || rule(t) || (t.starts_with("<!--") && t.ends_with("-->"));
        prev_blank = false;
    }
    out
}

/// The columns a line is indented by: a space one, a tab to the next 4.
fn columns(raw: &str) -> usize {
    let mut n = 0;
    for b in raw.bytes() {
        match b {
            b' ' => n += 1,
            b'\t' => n += 4 - n % 4,
            _ => break,
        }
    }
    n
}

/// Whether a line, indented 0 to 3, starts a block that ends a quote's
/// lazy paragraph: a `<!--`, a fence, a heading, a thematic break or a
/// list item.
fn starts_block(raw: &str) -> bool {
    if columns(raw) >= 4 {
        return false;
    }
    let t = raw.trim_start();
    t.starts_with("<!--") || opens(t).is_some() || heading(t) || rule(t) || item(t)
}

/// An ATX heading: 1 to 6 `#`, then a space or the line's end.
fn heading(t: &str) -> bool {
    let n = t.bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&n) && t[n..].chars().next().is_none_or(|c| c == ' ' || c == '\t')
}

/// A thematic break: three or more of one of `-`, `*` and `_`, spaces
/// between.
fn rule(t: &str) -> bool {
    let marks: Vec<u8> = t.bytes().filter(|b| *b != b' ' && *b != b'\t').collect();
    marks.len() >= 3
        && matches!(marks[0], b'-' | b'*' | b'_')
        && marks.iter().all(|b| *b == marks[0])
}

/// A list item's first line: `-`, `*` or `+`, or 1 to 9 digits and `.` or
/// `)`, then a space or the line's end.
fn item(t: &str) -> bool {
    let after = match t.as_bytes().first() {
        Some(b'-' | b'*' | b'+') => &t[1..],
        _ => {
            let n = t.bytes().take_while(u8::is_ascii_digit).count();
            if !(1..=9).contains(&n) {
                return false;
            }
            match t.as_bytes().get(n) {
                Some(b'.' | b')') => &t[n + 1..],
                _ => return false,
            }
        }
    };
    after.is_empty() || after.starts_with(' ') || after.starts_with('\t')
}

/// The fence a line opens, as CommonMark and GitHub read it: three or more
/// backticks or tildes, indented 0 to 3 spaces, and a backtick fence's
/// words after it hold no backtick. It closes at a line of the same
/// character, at least as many, indented 0 to 3 spaces, with nothing but
/// spaces and tabs after them. A fence inside a list item's indent is read
/// by these rules alone, not by the item's.
fn opens(text: &str) -> Option<(u8, usize)> {
    let c = *text
        .as_bytes()
        .first()
        .filter(|c| **c == b'`' || **c == b'~')?;
    let n = text.bytes().take_while(|b| *b == c).count();
    (n >= 3 && (c == b'~' || !text[n..].contains('`'))).then_some((c, n))
}

/// The code spans of one line: a run of backticks to the next run of the
/// same length, a backslash-escaped backtick opening none. A code span
/// that crosses a line end, which CommonMark allows, is a known departure:
/// it is read as no span. A factory marker stands alone on its line, so
/// neither reaches one.
fn spans(line: &str, at: usize, out: &mut Vec<(usize, usize)>) {
    let bytes = line.as_bytes();
    let run = |from: usize| bytes[from..].iter().take_while(|b| **b == b'`').count();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        // A backtick after an odd run of backslashes is a literal one, and
        // opens nothing.
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
        MarkerKind::Declined => Marker::Declined {
            question: CommentId::new(get("question")?),
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
