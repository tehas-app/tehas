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
//! What is no marker, as GitHub renders it: one inside a block quote
//! (on a `>` line, or on a line that lazily continues the quote's
//! paragraph), one inside fenced or indented code, and one that may be
//! inside a code span: between two backtick runs of one length in its
//! paragraph, or its line for a heading or a table row. The reader reads
//! the text's blocks as CommonMark and GitHub's tables do, as far as a
//! marker's place needs (see `dead`): a marker in an HTML block or a
//! paragraph, at the text's level or in a list item, reads. A line ends at
//! a LF, a CRLF or a lone CR, a blank line is spaces and tabs alone, a tab
//! partly used by a container keeps its other columns, and a byte order
//! mark at the text's start is no part of it.
//!
//! A marker reads only where GitHub keeps it as hidden HTML. So it is no
//! marker either where GitHub shows its words: after a backslash that
//! escapes its `<` (an odd run of them), or in a link's or an image's
//! destination or title, or in a paragraph that opens as a link reference
//! definition.
//!
//! Code spans are not paired: what GitHub reads first (inline HTML, an
//! autolink, a link's destination or title) may hold a backtick that pairs
//! nothing, so the reader takes every marker between two runs of one
//! length for code. Where the reader departs from GitHub it is the safe
//! way: such a marker GitHub may hide, the reader does not read. So too a
//! link is taken without its `[`, a definition without its rest, and past
//! [`MAX_NESTING`] open containers all is dead.
//!
//! `tests/markers_oracle.rs` holds the reader to a CommonMark and GFM
//! parser, comrak, over a recorded corpus and seeded random bodies: no
//! marker read where GitHub shows code, a quote or words, and the known
//! departures, each the safe way, named there.

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

/// [`read`], beside each marker read the bytes of the body it stands on,
/// None for one whose words stay in the text (bare, or a wrapper with no
/// close), and the bytes every wrapper of this text's markers holds, read
/// or not, a wrapper with no close to its line's end.
fn located(
    body: &str,
    item: Option<BlueprintNo>,
) -> (Read, Vec<Option<(usize, usize)>>, Vec<(usize, usize)>) {
    let rows = table::marker_rows();
    let lines = lines(body);
    let dead = dead(body, &lines);
    let mut found = wrapped(body, &lines, &dead, &rows, item);
    let bare = bare(body, &lines, &dead, &found, &rows, item);
    found.extend(bare);
    found.sort_by_key(|f| f.at);
    let spans: Vec<(usize, usize)> = found.iter().filter_map(|f| f.span).collect();
    let held: Vec<(usize, usize)> = found
        .iter()
        .filter(|f| {
            f.marker
                .as_ref()
                .is_some_and(|m| !matches!(m, Marker::Bare { .. }))
        })
        .map(|f| f.span.unwrap_or(f.shadow))
        .collect();
    let (mut markers, at): (Vec<Marker>, Vec<Option<(usize, usize)>>) = found
        .into_iter()
        .filter_map(|f| Some((f.marker?, f.span)))
        .unzip();
    twice(&mut markers, &rows);
    let read = Read {
        text: without(body, &lines, &spans),
        markers,
    };
    (read, at, held)
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
    let (read, at, _) = located(body, item);
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
/// that would have to be the first line, a text to replace whose bytes
/// overlap a marker's wrapper, read or not, or a result that does not
/// read back as asked.
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
    let (before, at, held) = located(body, item);
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
            let e = s + before.text.len();
            if held.iter().any(|(a, b)| s < *b && *a < e) {
                return Err(unsupported);
            }
            cuts.push((s, e, t.to_string()));
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

/// `body` with what GitHub renders as no text of a paragraph blanked to
/// spaces, line ends kept: quoted lines, fenced and indented code, code
/// spans and HTML comments. What a reader of GitHub's closing syntax may
/// take as said.
///
/// The blanked part is the union of the reader's dead parts and the line
/// rules this used before the reader parsed blocks, so it only grows:
/// more blanked only adds a closing line. The line rules hold where the
/// reader sees no quote or code, an HTML block or a list item among them.
pub fn live_text(body: &str) -> String {
    let mut gone = dead(body, &lines(body));
    gone.extend(line_rules(body));
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
            if c != '\n' && within(&gone, i) {
                ' '
            } else {
                c
            }
        })
        .collect()
}

/// What [`live_text`] blanked by lines alone, over lines split at LF and,
/// for its indented lines, at any line end too: a fence's lines, a line
/// whose first character past white space is `>`, a code span paired on
/// one line, and every indented line at the text's start or after a blank
/// line.
fn line_rules(body: &str) -> Vec<(usize, usize)> {
    let lf = lf_lines(body);
    let mut out = Vec::new();
    let mut fence: Option<(u8, usize)> = None;
    for l in &lf {
        let raw = &body[l.start..l.end];
        let indent = raw.len() - raw.trim_start_matches(' ').len();
        let text = (indent <= 3).then(|| &raw[indent..]);
        match fence {
            Some((c, n)) => {
                out.push((l.start, l.next));
                let closes = text.is_some_and(|t| {
                    let run = t.bytes().take_while(|b| *b == c).count();
                    run >= n && t[run..].bytes().all(|b| b == b' ' || b == b'\t')
                });
                if closes {
                    fence = None;
                }
            }
            None => match text.and_then(opens) {
                Some(open) => {
                    out.push((l.start, l.next));
                    fence = Some(open);
                }
                None if raw.trim_start().starts_with('>') => out.push((l.start, l.next)),
                None => paired(raw, l.start, &mut out),
            },
        }
    }
    for set in [lf, lines(body)] {
        let mut code_may_open = true;
        for l in set {
            let raw = &body[l.start..l.end];
            if raw.trim().is_empty() {
                code_may_open = true;
                continue;
            }
            if raw.starts_with("    ") || raw.starts_with('\t') {
                if code_may_open {
                    out.push((l.start, l.next));
                }
            } else {
                code_may_open = false;
            }
        }
    }
    out
}

/// Lines split at LF alone, a CR before it no part of the line's text.
fn lf_lines(body: &str) -> Vec<Line> {
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

/// The code spans of one line, paired: a run of backticks to the next run
/// of the same length, one after an odd run of backslashes opening none.
fn paired(line: &str, at: usize, out: &mut Vec<(usize, usize)>) {
    let bytes = line.as_bytes();
    let run = |from: usize| bytes[from..].iter().take_while(|b| **b == b'`').count();
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' || escaped(bytes, i) {
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
        // A line ends at a LF, a CRLF or a lone CR, as CommonMark has it.
        let (end, next) = match body[start..].find(['\n', '\r']) {
            None => (body.len(), body.len()),
            Some(i) if body[start + i..].starts_with("\r\n") => (start + i, start + i + 2),
            Some(i) => (start + i, start + i + 1),
        };
        out.push(Line { start, end, next });
        start = next;
    }
    out
}

/// The parts of the text that hold no marker, as GitHub renders it: every
/// line inside a block quote (its own `>` lines and the lazy lines that
/// continue its paragraph), every line of fenced or indented code with its
/// fences, and the code spans of a line.
///
/// It reads the text's blocks as CommonMark and GitHub's tables do, line by
/// line, as far as a marker's place needs: the open quotes and list items,
/// each item by its content's indent; and the leaf a line is in: a
/// paragraph, a table, fenced or indented code, or an HTML block. A marker
/// in an HTML block or a paragraph at the text's level or in a list item
/// reads. One pass, each line looked at once per open container.
fn dead(body: &str, lines: &[Line]) -> Vec<(usize, usize)> {
    let mut out = Vec::new();
    let mut open: Vec<Container> = Vec::new();
    let mut leaf = Leaf::None;
    // The paragraph being read, from its first line's text to its last's
    // end: its code spans may cross line ends.
    let mut para_at: Option<(usize, usize)> = None;
    // Which list item the paragraph is the first content of, if any.
    let mut para_first: Option<usize> = None;
    let flush = |para_at: &mut Option<(usize, usize)>, out: &mut Vec<(usize, usize)>| {
        if let Some((s, e)) = para_at.take() {
            inline(&body[s..e], s, true, out);
        }
    };
    for l in lines {
        let line = &body[l.start..l.end];
        // A byte order mark at the text's start is no part of it.
        let bom = if l.start == 0 && line.starts_with('\u{feff}') {
            3
        } else {
            0
        };
        let mut at = Cursor::at(bom, 0);
        // The containers this line goes on.
        let mut matched = 0;
        for c in &open {
            match c {
                Container::Quote => {
                    let (n, after) = at.indent(line);
                    if n <= 3 && line[after.byte..].starts_with('>') {
                        at = Cursor::at(after.byte + 1, after.col + 1).skip_one_space(line);
                        matched += 1;
                    } else {
                        break;
                    }
                }
                Container::Item { indent, empty } => {
                    let (n, _) = at.indent(line);
                    // An item may start with one blank line at most: one
                    // still empty at a blank line closes.
                    if blank(&line[at.byte..]) && !*empty {
                        matched += 1;
                    } else if !blank(&line[at.byte..]) && n >= *indent {
                        at = at.advance(line, *indent);
                        matched += 1;
                    } else {
                        break;
                    }
                }
            }
        }
        let rest = &line[at.byte..];
        if matched < open.len() {
            let lazy = (para(leaf) || leaf == Leaf::Deep) && !blank(rest) && !interrupts(at, line);
            if lazy {
                if leaf == Leaf::Deep || open.contains(&Container::Quote) {
                    out.push((l.start, l.next));
                } else {
                    // The lazy line is now the paragraph's last, which a
                    // delimiter row may make a table's head.
                    leaf = Leaf::Para(cells(&line[at.indent(line).1.byte..]).len());
                    let s = para_at.map_or(l.start + at.byte, |(s, _)| s);
                    para_at = Some((s, l.end));
                }
                continue;
            }
            open.truncate(matched);
            leaf = Leaf::None;
            flush(&mut para_at, &mut out);
        }
        // The leaf this line continues.
        match leaf {
            Leaf::Fence(c, n) => {
                out.push((l.start, l.next));
                let (ind, after) = at.indent(line);
                let t = &line[after.byte..];
                let run = t.bytes().take_while(|b| *b == c).count();
                if ind <= 3 && run >= n && t[run..].bytes().all(|b| b == b' ' || b == b'\t') {
                    leaf = Leaf::None;
                }
                continue;
            }
            Leaf::Indented => {
                let (ind, _) = at.indent(line);
                if blank(rest) || ind >= 4 {
                    out.push((l.start, l.next));
                    continue;
                }
                leaf = Leaf::None;
            }
            Leaf::Html(end) => {
                if end == HtmlEnd::Blank && blank(rest) {
                    leaf = Leaf::None;
                    continue;
                }
                if open.contains(&Container::Quote) {
                    out.push((l.start, l.next));
                }
                if html_ends(end, rest) {
                    leaf = Leaf::None;
                }
                continue;
            }
            Leaf::Deep => {
                out.push((l.start, l.next));
                continue;
            }
            _ => {}
        }
        // New containers, as many as the line opens, up to the cap.
        let mut deep = false;
        let before = open.len();
        // A thematic break runs to the line's end: only a start in the
        // line's tail of one mark can be one, so each item tried costs
        // nothing more.
        let tail = rule_tail(line);
        let is_rule = |from: usize| from >= tail && rule(&line[from..]);
        loop {
            let (ind, after) = at.indent(line);
            if ind >= 4 {
                break;
            }
            let t = &line[after.byte..];
            let opens_one = t.starts_with('>')
                || item(t).is_some_and(|width| {
                    let empty = blank(&t[width..]);
                    (!para(leaf)
                        || (!empty
                            && (matches!(t.as_bytes()[0], b'-' | b'*' | b'+')
                                || t.starts_with("1.")
                                || t.starts_with("1)"))))
                        && !is_rule(after.byte)
                });
            if opens_one && open.len() >= MAX_NESTING {
                deep = true;
                break;
            }
            // A container that opens in an item gives it content.
            if let Some(Container::Item { empty, .. }) = open.last_mut().filter(|_| opens_one) {
                *empty = false;
            }
            if t.starts_with('>') {
                open.push(Container::Quote);
                at = Cursor::at(after.byte + 1, after.col + 1).skip_one_space(line);
                leaf = Leaf::None;
                continue;
            }
            if let Some(width) = item(t) {
                let empty = blank(&t[width..]);
                let may = !para(leaf)
                    || (!empty
                        && (matches!(t.as_bytes()[0], b'-' | b'*' | b'+')
                            || t.starts_with("1.")
                            || t.starts_with("1)")));
                if may && !is_rule(after.byte) {
                    let marker = Cursor::at(after.byte + width, after.col + width);
                    let (spaces, content) = marker.indent(line);
                    let indent = if empty || spaces > 4 {
                        marker.col + 1 - at.col
                    } else {
                        content.col - at.col
                    };
                    open.push(Container::Item { indent, empty });
                    at = if empty || spaces > 4 {
                        marker.skip_one_space(line)
                    } else {
                        content
                    };
                    leaf = Leaf::None;
                    continue;
                }
            }
            break;
        }
        if deep {
            // Past the cap, the rest of the line and all inside it is dead.
            flush(&mut para_at, &mut out);
            leaf = Leaf::Deep;
            out.push((l.start, l.next));
            continue;
        }
        if open.len() > before {
            // A new container ends the paragraph before it.
            flush(&mut para_at, &mut out);
        }
        let rest = &line[at.byte..];
        let quoted = open.contains(&Container::Quote);
        let first = match open.last() {
            Some(Container::Item { empty, .. }) => *empty || open.len() > before,
            _ => false,
        };
        if !blank(rest) {
            for c in open.iter_mut() {
                if let Container::Item { empty, .. } = c {
                    *empty = false;
                }
            }
        }
        if blank(rest) {
            // A paragraph of link reference definitions alone leaves no
            // block: an item that held nothing else is empty again, and the
            // next blank line closes it.
            let defs = para_at.is_some_and(|(s, e)| definition(&body[s..e]));
            if let Some(Container::Item { empty, .. }) =
                para_first.filter(|_| defs).and_then(|i| open.get_mut(i))
            {
                *empty = true;
            }
            leaf = Leaf::None;
            flush(&mut para_at, &mut out);
            continue;
        }
        let (ind, after) = at.indent(line);
        let t = &line[after.byte..];
        let mut html_line = false;
        let dead_line = if ind >= 4 {
            if para(leaf) {
                leaf = Leaf::Para(cells(t).len());
                false
            } else {
                leaf = Leaf::Indented;
                true
            }
        } else if let Some((c, n)) = opens(t) {
            leaf = Leaf::Fence(c, n);
            true
        } else if let Some(end) = html_start(t, para(leaf)) {
            html_line = true;
            leaf = if html_ends(end, t) {
                Leaf::None
            } else {
                Leaf::Html(end)
            };
            false
        } else if heading(t) || (para(leaf) && setext(t)) || rule(t) {
            leaf = Leaf::None;
            false
        } else if matches!(leaf, Leaf::Para(n) if delimiter_row(t) == Some(n)) {
            // As GFM's tables have it: the paragraph's last line becomes the
            // table's head, the lines before it stay a paragraph.
            leaf = Leaf::Table;
            false
        } else if leaf == Leaf::Table {
            false
        } else {
            leaf = Leaf::Para(cells(t).len());
            false
        };
        if para(leaf) && !quoted {
            if para_at.is_none() {
                para_first = first.then(|| open.len() - 1);
            }
            let s = para_at.map_or(l.start + at.byte, |(s, _)| s);
            para_at = Some((s, l.end));
            continue;
        }
        flush(&mut para_at, &mut out);
        if dead_line || quoted {
            out.push((l.start, l.next));
        } else {
            inline(rest, l.start + at.byte, !html_line, &mut out);
        }
    }
    flush(&mut para_at, &mut out);
    out
}

/// A container a line can be in: a block quote, or a list item whose
/// content is indented by `indent` columns past its container's, `empty`
/// while no line has given it content.
#[derive(Clone, Copy, PartialEq, Eq)]
enum Container {
    Quote,
    Item { indent: usize, empty: bool },
}

/// How many containers, quotes and list items, may be open at once. A
/// container that would go past it makes the rest of its line, and every
/// line after it while the open ones go on, dead: the safe way. It keeps
/// each line's work bounded, so a body of nested markers reads in linear
/// time.
const MAX_NESTING: usize = 100;

/// The block a line is in, past its containers. `Para` is a paragraph with
/// the count of cells of its last line, which a table's delimiter row of
/// as many cells makes a table's head. `Deep` is what is past
/// [`MAX_NESTING`].
#[derive(Clone, Copy, PartialEq, Eq)]
enum Leaf {
    None,
    Para(usize),
    Table,
    Deep,
    Fence(u8, usize),
    Indented,
    Html(HtmlEnd),
}

/// A paragraph, of one line or more.
fn para(leaf: Leaf) -> bool {
    matches!(leaf, Leaf::Para(_))
}

/// How an HTML block ends: at a line holding these words, or at a blank
/// line.
#[derive(Clone, Copy, PartialEq, Eq)]
enum HtmlEnd {
    Words(&'static [&'static str]),
    Blank,
}

/// A place in a line: its byte and its column, a tab going to the next 4.
#[derive(Clone, Copy)]
struct Cursor {
    byte: usize,
    col: usize,
    /// The columns left of a tab partly used: spaces before `byte`.
    extra: usize,
}

impl Cursor {
    fn at(byte: usize, col: usize) -> Cursor {
        Cursor {
            byte,
            col,
            extra: 0,
        }
    }

    /// The columns of white space from here, a partly used tab's first,
    /// and where it ends.
    fn indent(self, line: &str) -> (usize, Cursor) {
        let mut c = Cursor::at(self.byte, self.col + self.extra);
        for b in line.as_bytes()[self.byte..].iter() {
            match b {
                b' ' => c.col += 1,
                b'\t' => c.col += 4 - c.col % 4,
                _ => break,
            }
            c.byte += 1;
        }
        (c.col - self.col, c)
    }

    /// Past `n` columns of white space; of a tab that crosses the mark,
    /// the columns past it are kept, as cmark keeps them.
    fn advance(self, line: &str, n: usize) -> Cursor {
        let goal = self.col + n;
        let take = self.extra.min(n);
        let mut c = Cursor {
            byte: self.byte,
            col: self.col + take,
            extra: self.extra - take,
        };
        if c.extra > 0 {
            return c;
        }
        for b in line.as_bytes()[self.byte..].iter() {
            if c.col >= goal {
                break;
            }
            match b {
                b' ' => c.col += 1,
                b'\t' => {
                    let w = 4 - c.col % 4;
                    c.byte += 1;
                    if c.col + w > goal {
                        c.extra = c.col + w - goal;
                        c.col = goal;
                        return c;
                    }
                    c.col += w;
                    continue;
                }
                _ => break,
            }
            c.byte += 1;
        }
        c
    }

    /// Past one column of white space, if one is here: a space, or the
    /// first column of a tab, the rest of the tab kept.
    fn skip_one_space(self, line: &str) -> Cursor {
        if self.extra > 0 {
            return Cursor {
                byte: self.byte,
                col: self.col + 1,
                extra: self.extra - 1,
            };
        }
        match line.as_bytes().get(self.byte) {
            Some(b' ') => Cursor::at(self.byte + 1, self.col + 1),
            Some(b'\t') => Cursor {
                byte: self.byte + 1,
                col: self.col + 1,
                extra: 3 - self.col % 4,
            },
            _ => self,
        }
    }
}

/// A line of spaces and tabs alone. Other white space, a no-break space
/// among it, is text.
fn blank(t: &str) -> bool {
    t.bytes().all(|b| b == b' ' || b == b'\t')
}

/// Whether the rest of a line, past the containers it matched, starts a
/// block that ends a paragraph, so it is no lazy continuation of one.
fn interrupts(at: Cursor, line: &str) -> bool {
    let (ind, after) = at.indent(line);
    if ind >= 4 {
        return false;
    }
    let t = &line[after.byte..];
    // As cmark decides it, the block a line would open is tried against
    // the last container the line matched, which is no paragraph: any list
    // item and any HTML block opens, where inside the paragraph's own
    // container they would not.
    t.starts_with('>')
        || opens(t).is_some()
        || heading(t)
        || rule(t)
        || html_start(t, false).is_some()
        || item(t).is_some()
}

/// The fence a line opens, as CommonMark reads it: three or more backticks
/// or tildes, and a backtick fence's words after them hold no backtick. It
/// closes at a line of the same character, at least as many, indented 0 to
/// 3 columns, with nothing but spaces and tabs after them.
fn opens(text: &str) -> Option<(u8, usize)> {
    let c = *text
        .as_bytes()
        .first()
        .filter(|c| **c == b'`' || **c == b'~')?;
    let n = text.bytes().take_while(|b| *b == c).count();
    (n >= 3 && (c == b'~' || !text[n..].contains('`'))).then_some((c, n))
}

/// An ATX heading: 1 to 6 `#`, then a space, a tab or the line's end.
fn heading(t: &str) -> bool {
    let n = t.bytes().take_while(|b| *b == b'#').count();
    (1..=6).contains(&n) && t[n..].chars().next().is_none_or(|c| c == ' ' || c == '\t')
}

/// A setext heading's underline: `=` or `-` alone, spaces after.
fn setext(t: &str) -> bool {
    let t = t.trim_end_matches([' ', '\t']);
    !t.is_empty() && (t.bytes().all(|b| b == b'=') || t.bytes().all(|b| b == b'-'))
}

/// A thematic break: three or more of one of `-`, `*` and `_`, spaces and
/// tabs between.
fn rule(t: &str) -> bool {
    let marks: Vec<u8> = t.bytes().filter(|b| *b != b' ' && *b != b'\t').collect();
    marks.len() >= 3
        && matches!(marks[0], b'-' | b'*' | b'_')
        && marks.iter().all(|b| *b == marks[0])
}

/// Where the line's tail of white space and one of `-`, `*` and `_` starts.
fn rule_tail(line: &str) -> usize {
    let b = line.as_bytes();
    let mut mark = None;
    let mut i = b.len();
    while i > 0 {
        let c = b[i - 1];
        let fits = match mark {
            _ if c == b' ' || c == b'\t' => true,
            None if matches!(c, b'-' | b'*' | b'_') => {
                mark = Some(c);
                true
            }
            m => m == Some(c),
        };
        if !fits {
            break;
        }
        i -= 1;
    }
    i
}

/// A list item's marker: `-`, `*` or `+`, or 1 to 9 digits and `.` or `)`,
/// then a space, a tab or the line's end. Its width in bytes.
fn item(t: &str) -> Option<usize> {
    let width = match t.as_bytes().first() {
        Some(b'-' | b'*' | b'+') => 1,
        _ => {
            let n = t.bytes().take_while(u8::is_ascii_digit).count();
            if !(1..=9).contains(&n) || !matches!(t.as_bytes().get(n), Some(b'.' | b')')) {
                return None;
            }
            n + 1
        }
    };
    matches!(t.as_bytes().get(width), None | Some(b' ' | b'\t')).then_some(width)
}

/// A GFM table's delimiter row: cells of `-`, with `:` at either end,
/// between pipes. Its count of cells.
fn delimiter_row(t: &str) -> Option<usize> {
    let all = cells(t);
    let ok = (t.contains('|') || all.len() == 1)
        && !all.is_empty()
        && all.iter().all(|c| {
            let c = c.strip_prefix(':').unwrap_or(c);
            let c = c.strip_suffix(':').unwrap_or(c);
            !c.is_empty() && c.bytes().all(|b| b == b'-')
        });
    ok.then_some(all.len())
}

/// A table row's cells, as GFM splits one: at each `|` no backslash
/// escapes, a leading and a trailing pipe dropped.
fn cells(t: &str) -> Vec<&str> {
    let t = t.trim_matches([' ', '\t']);
    let inner = t.strip_prefix('|').unwrap_or(t);
    let inner = match inner.strip_suffix('|') {
        Some(i) if !i.ends_with('\\') => i,
        _ => inner,
    };
    let mut out = Vec::new();
    let mut from = 0;
    let b = inner.as_bytes();
    for (i, c) in b.iter().enumerate() {
        if *c == b'|' && (i == 0 || b[i - 1] != b'\\') {
            out.push(inner[from..i].trim_matches([' ', '\t']));
            from = i + 1;
        }
    }
    out.push(inner[from..].trim_matches([' ', '\t']));
    out
}

/// The block names of CommonMark's HTML blocks of the sixth kind.
const BLOCK_TAGS: &[&str] = &[
    "address",
    "article",
    "aside",
    "base",
    "basefont",
    "blockquote",
    "body",
    "caption",
    "center",
    "col",
    "colgroup",
    "dd",
    "details",
    "dialog",
    "dir",
    "div",
    "dl",
    "dt",
    "fieldset",
    "figcaption",
    "figure",
    "footer",
    "form",
    "frame",
    "frameset",
    "h1",
    "h2",
    "h3",
    "h4",
    "h5",
    "h6",
    "head",
    "header",
    "hr",
    "html",
    "iframe",
    "legend",
    "li",
    "link",
    "main",
    "menu",
    "menuitem",
    "nav",
    "noframes",
    "ol",
    "optgroup",
    "option",
    "p",
    "param",
    "search",
    "section",
    "summary",
    "table",
    "tbody",
    "td",
    "tfoot",
    "th",
    "thead",
    "title",
    "tr",
    "track",
    "ul",
];

/// The HTML block a line, indented 0 to 3, opens, by CommonMark's seven
/// kinds, and how it ends. The seventh, a lone tag, cannot interrupt a
/// paragraph.
fn html_start(t: &str, in_para: bool) -> Option<HtmlEnd> {
    if !t.starts_with('<') {
        return None;
    }
    let low = t.to_ascii_lowercase();
    // The first kind ends at any of its four close tags, as cmark has it.
    for open in ["<script", "<pre", "<style", "<textarea"] {
        if let Some(after) = low.strip_prefix(open) {
            if after.is_empty() || after.starts_with([' ', '\t', '>']) {
                return Some(HtmlEnd::Words(&[
                    "</script>",
                    "</pre>",
                    "</style>",
                    "</textarea>",
                ]));
            }
        }
    }
    if low.starts_with("<!--") {
        return Some(HtmlEnd::Words(&["-->"]));
    }
    if low.starts_with("<?") {
        return Some(HtmlEnd::Words(&["?>"]));
    }
    if low.starts_with("<![cdata[") {
        return Some(HtmlEnd::Words(&["]]>"]));
    }
    if low.starts_with("<!") && low.as_bytes().get(2).is_some_and(u8::is_ascii_alphabetic) {
        return Some(HtmlEnd::Words(&[">"]));
    }
    let name_at = if low.starts_with("</") { 2 } else { 1 };
    let name: String = low[name_at..]
        .chars()
        .take_while(|c| c.is_ascii_alphanumeric() || *c == '-')
        .collect();
    let after = &low[name_at + name.len()..];
    if BLOCK_TAGS.contains(&name.as_str())
        && (after.is_empty() || after.starts_with([' ', '\t', '>']) || after.starts_with("/>"))
    {
        return Some(HtmlEnd::Blank);
    }
    // The seventh kind: a whole open or close tag alone on the line. The
    // first kind's open tags are taken above, so, as in cmark, its close
    // tags (`</pre>`) are of this kind.
    let lone = tag_end(&low).is_some_and(|end| blank(&low[end..]));
    (!in_para && lone).then_some(HtmlEnd::Blank)
}

/// Where a whole open tag or close tag at the start of `t` ends, as
/// CommonMark spells one: `<name`, attributes, white space, an optional
/// `/`, then `>`; or `</name`, white space, `>`. None for anything else.
fn tag_end(t: &str) -> Option<usize> {
    let b = t.as_bytes();
    let ws = |i: &mut usize| {
        let from = *i;
        while b.get(*i).is_some_and(|c| matches!(c, b' ' | b'\t')) {
            *i += 1;
        }
        *i > from
    };
    let name = |i: &mut usize| {
        if !b.get(*i).is_some_and(u8::is_ascii_alphabetic) {
            return false;
        }
        while b
            .get(*i)
            .is_some_and(|c| c.is_ascii_alphanumeric() || *c == b'-')
        {
            *i += 1;
        }
        true
    };
    let mut i = 1;
    if b.get(1) == Some(&b'/') {
        i = 2;
        if !name(&mut i) {
            return None;
        }
        ws(&mut i);
        return (b.get(i) == Some(&b'>')).then_some(i + 1);
    }
    if !name(&mut i) {
        return None;
    }
    loop {
        let spaced = ws(&mut i);
        match b.get(i) {
            Some(b'>') => return Some(i + 1),
            Some(b'/') => return (b.get(i + 1) == Some(&b'>')).then_some(i + 2),
            Some(c) if spaced && (c.is_ascii_alphabetic() || matches!(c, b'_' | b':')) => {
                while b.get(i).is_some_and(|c| {
                    c.is_ascii_alphanumeric() || matches!(c, b'_' | b'.' | b':' | b'-')
                }) {
                    i += 1;
                }
                let at = i;
                ws(&mut i);
                if b.get(i) == Some(&b'=') {
                    i += 1;
                    ws(&mut i);
                    match b.get(i) {
                        Some(q @ (b'"' | b'\'')) => {
                            let close = t[i + 1..].find(*q as char)?;
                            i += close + 2;
                        }
                        Some(_) => {
                            let from = i;
                            while b.get(i).is_some_and(|c| {
                                !matches!(
                                    c,
                                    b' ' | b'\t' | b'"' | b'\'' | b'=' | b'<' | b'>' | b'`'
                                )
                            }) {
                                i += 1;
                            }
                            if i == from {
                                return None;
                            }
                        }
                        None => return None,
                    }
                } else {
                    i = at;
                }
            }
            _ => return None,
        }
    }
}

/// Whether a line of an HTML block ends it.
fn html_ends(end: HtmlEnd, line: &str) -> bool {
    match end {
        HtmlEnd::Words(words) => {
            let low = line.to_ascii_lowercase();
            words.iter().any(|w| low.contains(w))
        }
        HtmlEnd::Blank => false,
    }
}

/// What of a run of inline text holds no marker: the code spans of
/// [`spans`]; and where `prose` (a paragraph, a heading or a table row; not
/// an HTML block, whose text is raw), what GitHub shows as words: a `<!--`
/// after an odd run of backslashes, which escape its `<`; a link's or an
/// image's destination and title; and the whole of a paragraph that opens
/// as a link reference definition.
fn inline(text: &str, at: usize, prose: bool, out: &mut Vec<(usize, usize)>) {
    spans(text, at, out);
    if !prose {
        return;
    }
    if definition(text) {
        out.push((at, at + text.len()));
        return;
    }
    let b = text.as_bytes();
    for (i, _) in text.match_indices("<!--") {
        if escaped(b, i) {
            // Its words are shown, and are no bare marker either.
            let end = text[i..].find("-->").map_or(text.len(), |e| i + e + 3);
            out.push((at + i, at + end));
        }
    }
    for (i, _) in text.match_indices("](") {
        if let Some(end) = destination(b, i + 2).filter(|_| !escaped(b, i)) {
            out.push((at + i + 2, at + end));
            if end == b.len() {
                break;
            }
        }
    }
}

/// Whether the byte at `i` follows an odd run of backslashes.
fn escaped(b: &[u8], i: usize) -> bool {
    b[..i].iter().rev().take_while(|c| **c == b'\\').count() % 2 == 1
}

/// Whether a paragraph opens as a link reference definition: `[`, a label
/// with no unescaped `]`, then `]:`. More than CommonMark takes, the safe
/// way: the rest of the definition is not checked.
fn definition(text: &str) -> bool {
    let t = text.trim_start_matches([' ', '\t']).as_bytes();
    if t.first() != Some(&b'[') {
        return false;
    }
    let close = (1..t.len()).find(|i| t[*i] == b']' && !escaped(t, *i));
    close.is_some_and(|i| i > 1 && t.get(i + 1) == Some(&b':'))
}

/// Where an inline link's destination and title, from the byte after
/// `](`, end with their `)`: a destination in `<` and `>` or one of no
/// white space with balanced parentheses, then a title in quotes or
/// parentheses. None when no link closes there. The `[` that opens the
/// link is not looked for, the safe way. Parentheses nested deeper than
/// cmark takes (32) make the rest of the text a destination, the safe way
/// too, which also keeps the scans linear.
fn destination(b: &[u8], from: usize) -> Option<usize> {
    let space = |i: &mut usize| {
        while b
            .get(*i)
            .is_some_and(|c| matches!(c, b' ' | b'\t' | b'\n' | b'\r'))
        {
            *i += 1;
        }
    };
    let mut i = from;
    space(&mut i);
    if b.get(i) == Some(&b'<') {
        i += 1;
        loop {
            match b.get(i)? {
                b'\n' | b'\r' | b'<' => return None,
                b'\\' => i += 2,
                b'>' => {
                    i += 1;
                    break;
                }
                _ => i += 1,
            }
        }
    } else {
        let mut depth = 0usize;
        while let Some(&c) = b.get(i) {
            match c {
                b'\\' => i += 2,
                b'(' if depth == 32 => return Some(b.len()),
                b'(' => {
                    depth += 1;
                    i += 1;
                }
                b')' if depth == 0 => break,
                b')' => {
                    depth -= 1;
                    i += 1;
                }
                c if c <= b' ' => break,
                _ => i += 1,
            }
        }
        if depth > 0 {
            return None;
        }
    }
    space(&mut i);
    let close = match b.get(i)? {
        b')' => return Some(i + 1),
        b'"' => b'"',
        b'\'' => b'\'',
        b'(' => b')',
        _ => return None,
    };
    i += 1;
    loop {
        match b.get(i)? {
            b'\\' => i += 2,
            c if *c == close => {
                i += 1;
                break;
            }
            b'(' if close == b')' => return None,
            _ => i += 1,
        }
    }
    space(&mut i);
    (b.get(i)? == &b')').then_some(i + 1)
}

/// What of a paragraph's text, or of one line, may be inside a code span:
/// for each length of backtick run, from the first run of it to the last,
/// across line ends too. A run after an odd run of backslashes counts at
/// its length and at one less. Whatever GitHub reads first (inline HTML, an
/// autolink, a link's destination) may hold a backtick that pairs nothing,
/// so no runs are paired here: a marker between two runs of one length is
/// no marker, which is the safe way. One pass over the text.
fn spans(text: &str, at: usize, out: &mut Vec<(usize, usize)>) {
    let bytes = text.as_bytes();
    // For each run length: where its first run starts, where its last
    // ends, and how many runs it has.
    let mut reach: std::collections::HashMap<usize, (usize, usize, usize)> = Default::default();
    let mut note = |len: usize, start: usize, end: usize| {
        if len > 0 {
            reach
                .entry(len)
                .and_modify(|r| (r.1, r.2) = (end, r.2 + 1))
                .or_insert((start, end, 1));
        }
    };
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] != b'`' {
            i += 1;
            continue;
        }
        let n = bytes[i..].iter().take_while(|b| **b == b'`').count();
        let slashes = bytes[..i].iter().rev().take_while(|b| **b == b'\\').count();
        note(n, i, i + n);
        if slashes % 2 == 1 {
            note(n - 1, i + 1, i + n);
        }
        i += n;
    }
    for (s, e, runs) in reach.into_values() {
        if runs > 1 {
            out.push((at + s, at + e));
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
