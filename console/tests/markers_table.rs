//! host/markers.tsv, the table of tehas's markers, read whole: one row per
//! kind but a project's own, each field of a known type, and each field the
//! text holds named in the row. The adapters' tests read the same file for
//! what each marker means (T4.2).

const TABLE: &str = include_str!("../../host/markers.tsv");

/// The kinds of the ports' MarkerKind, `Project` aside, in the table's order.
const KINDS: [&str; 10] = [
    "Question",
    "Blocked",
    "HoldCleared",
    "Park",
    "Finish",
    "LastAttempt",
    "Millwright",
    "TicketOf",
    "Link",
    "Declined",
];

/// The kinds that are a block: an open line, a payload, a close line.
const BLOCKS: [&str; 1] = ["LastAttempt"];

/// The field types the table's header defines, each without its arguments.
const TYPES: [&str; 8] = [
    "blueprint",
    "number",
    "numbers",
    "words",
    "word",
    "hex",
    "key",
    "text",
];

struct Row<'a> {
    kind: &'a str,
    open: &'a str,
    close: &'a str,
    fields: Vec<(&'a str, &'a str)>,
}

fn rows() -> Vec<Row<'static>> {
    rows_of(TABLE)
}

/// The rows of a table's text; a row of other than four columns panics.
fn rows_of(text: &str) -> Vec<Row<'_>> {
    text.lines()
        .filter(|l| !l.starts_with('#') && !l.is_empty())
        .map(|line| {
            let cols: Vec<&str> = line.split('\t').collect();
            assert_eq!(cols.len(), 4, "four columns: {line:?}");
            let fields = cols[3]
                .split("; ")
                .filter(|f| !f.is_empty())
                .map(|f| {
                    let (name, ty) = f.split_once(' ').unwrap_or_else(|| panic!("{f:?}"));
                    (name, ty)
                })
                .collect();
            Row {
                kind: cols[0],
                open: cols[1],
                close: cols[2],
                fields,
            }
        })
        .collect()
}

/// The names a text holds as `{name}`.
fn placeholders(text: &str) -> Vec<&str> {
    text.split('{')
        .skip(1)
        .map(|part| part.split('}').next().unwrap_or_default())
        .collect()
}

#[test]
fn every_kind_but_a_projects_own_has_one_row_in_order() {
    let kinds: Vec<&str> = rows().iter().map(|r| r.kind).collect();
    assert_eq!(kinds, KINDS);
}

#[test]
fn a_block_has_a_close_text_and_no_other_row_does() {
    for row in rows() {
        assert_eq!(
            !row.close.is_empty(),
            BLOCKS.contains(&row.kind),
            "{}",
            row.kind
        );
        assert!(
            row.open.starts_with("<!-- ") && row.open.ends_with(" -->"),
            "{}",
            row.open
        );
        if !row.close.is_empty() {
            assert!(row.close.starts_with("<!-- /"), "{}", row.close);
        }
    }
}

#[test]
fn each_field_has_a_known_type_and_appears_once() {
    for row in rows() {
        let mut names: Vec<&str> = Vec::new();
        for (name, ty) in &row.fields {
            let name = name.trim_end_matches('?');
            assert!(!names.contains(&name), "{} {name} twice", row.kind);
            names.push(name);
            let base = ty.split('(').next().unwrap_or_default();
            assert!(TYPES.contains(&base), "{} {name}: {ty}", row.kind);
        }
    }
}

#[test]
fn the_open_text_holds_each_field_it_names_and_every_other_is_a_blocks_payload() {
    for row in rows() {
        let held = placeholders(row.open);
        let names: Vec<&str> = row
            .fields
            .iter()
            .map(|(n, _)| n.trim_end_matches('?'))
            .collect();
        for name in &held {
            assert!(names.contains(name), "{} holds {{{name}}}", row.kind);
        }
        for (name, ty) in &row.fields {
            let name = name.trim_end_matches('?');
            if !held.contains(&name) {
                assert!(
                    BLOCKS.contains(&row.kind),
                    "{} names {name} and its text does not hold it",
                    row.kind
                );
                let base = ty.split('(').next().unwrap_or_default();
                assert!(
                    ["hex", "text"].contains(&base),
                    "{} {name}: {ty} is no payload",
                    row.kind
                );
            }
        }
    }
}

#[test]
fn a_row_of_three_columns_is_refused_and_a_comment_is_skipped() {
    assert!(std::panic::catch_unwind(|| rows_of("Blocked\t<!-- x -->\t")).is_err());
    assert_eq!(rows_of("# a note\nHoldCleared\t<!-- x -->\t\t").len(), 1);
}

/// What a reader makes of one marker's text, as the table's header rules.
#[derive(Debug, PartialEq)]
enum Read {
    /// The marker, its known fields in the row's order.
    Marker(Vec<(String, String)>),
    /// Not this row's marker at all: its wrapper is not there.
    NoMarker,
    /// The row's wrapper with what does not read inside it.
    Unreadable,
}

/// One inline marker of `row` in `text`, read as the header says: each field
/// of the row once and in order, no other field, each value of its type.
fn read(row: &Row, text: &str) -> Read {
    // The opening two words of the row's open text, then a space.
    let head: Vec<&str> = row.open.split_whitespace().take(2).collect();
    let head = format!("{} ", head.join(" "));
    let Some(rest) = text.strip_prefix(head.as_str()) else {
        return Read::NoMarker;
    };
    let Some(inside) = rest.split("-->").next().filter(|_| rest.contains("-->")) else {
        return Read::NoMarker;
    };
    let names: Vec<&str> = row
        .fields
        .iter()
        .map(|(n, _)| n.trim_end_matches('?'))
        .collect();
    let mut got: Vec<(usize, String, String)> = Vec::new();
    for token in inside.split_whitespace() {
        let Some((name, value)) = token.split_once('=') else {
            return Read::Unreadable;
        };
        let Some(at) = names.iter().position(|n| *n == name) else {
            return Read::Unreadable;
        };
        if got.last().is_some_and(|(last, _, _)| *last >= at) {
            return Read::Unreadable;
        }
        got.push((at, name.to_string(), value.to_string()));
    }
    for (at, (name, ty)) in row.fields.iter().enumerate() {
        let found = got.iter().find(|(i, _, _)| *i == at);
        let Some((_, _, value)) = found else {
            if name.ends_with('?') {
                continue;
            }
            return Read::Unreadable;
        };
        if !of_type(ty, value) {
            return Read::Unreadable;
        }
    }
    Read::Marker(got.into_iter().map(|(_, n, v)| (n, v)).collect())
}

/// Whether `value` is of the header's type `ty`, the inline types alone.
fn of_type(ty: &str, value: &str) -> bool {
    let (base, args) = match ty.split_once('(') {
        Some((b, a)) => (b, a.trim_end_matches(')')),
        None => (ty, ""),
    };
    let number = |v: &str, n: usize| {
        !v.is_empty()
            && v.len() <= n
            && v.bytes().all(|b| b.is_ascii_digit())
            && !v.starts_with('0')
    };
    match base {
        "blueprint" => number(value, 9),
        "number" => number(value, args.parse().unwrap_or(0)),
        "numbers" => {
            let (range, n) = args.split_once(',').unwrap_or_default();
            let (lo, hi) = range.split_once('-').unwrap_or_default();
            let parts: Vec<&str> = value.split(',').collect();
            let count = lo.parse().unwrap_or(1)..=hi.parse().unwrap_or(0);
            count.contains(&parts.len()) && parts.iter().all(|p| number(p, n.parse().unwrap_or(0)))
        }
        "words" => args.split('|').any(|w| w == value),
        "word" => {
            value.len() <= args.parse().unwrap_or(0)
                && value
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        }
        "hex" => {
            value.len() == args.parse().unwrap_or(0)
                && value
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
        }
        "key" => {
            (1..=args.parse().unwrap_or(0)).contains(&value.len())
                && value
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b"._:/-".contains(&b))
        }
        _ => false,
    }
}

fn row(kind: &str) -> Row<'static> {
    rows().into_iter().find(|r| r.kind == kind).unwrap()
}

/// A marker of `row` as its open text starts it, with `pairs` as its fields
/// in the order given: the cases are made from the table, not written out.
fn text(row: &Row, pairs: &[(&str, &str)]) -> String {
    let head: Vec<&str> = row.open.split_whitespace().take(2).collect();
    let mut out = head.join(" ");
    for (name, value) in pairs {
        out.push_str(&format!(" {name}={value}"));
    }
    out.push_str(" -->");
    out
}

fn fields(pairs: &[(&str, &str)]) -> Read {
    Read::Marker(
        pairs
            .iter()
            .map(|(n, v)| (n.to_string(), v.to_string()))
            .collect(),
    )
}

#[test]
fn a_ticket_key_is_a_number_a_key_or_a_path_and_one_that_breaks_the_pattern_is_unreadable() {
    let r = row("TicketOf");
    for key in ["123", "KEY-123", "org/repo:12", "a.b_c", &"k".repeat(128)] {
        assert_eq!(
            read(&r, &text(&r, &[("ticket", key)])),
            fields(&[("ticket", key)]),
            "{key}"
        );
    }
    for key in ["", "KEY#1", "a,b", &"k".repeat(129), "ä"] {
        assert_eq!(
            read(&r, &text(&r, &[("ticket", key)])),
            Read::Unreadable,
            "{key:?}"
        );
    }
    // White space ends the key: what follows is no field, so the marker
    // does not read.
    assert_eq!(read(&r, &text(&r, &[("ticket", "a b")])), Read::Unreadable);
}

#[test]
fn a_field_not_of_the_row_makes_any_marker_unreadable() {
    assert_eq!(
        read(
            &row("Link"),
            &text(&row("Link"), &[("blueprint", "12"), ("later", "x")])
        ),
        Read::Unreadable
    );
    assert_eq!(
        read(
            &row("TicketOf"),
            &text(&row("TicketOf"), &[("later", "x"), ("ticket", "KEY-1")])
        ),
        Read::Unreadable
    );
    assert_eq!(
        read(
            &row("Blocked"),
            &text(&row("Blocked"), &[("by", "3,4"), ("why", "later")])
        ),
        Read::Unreadable
    );
}

#[test]
fn remove_other_than_1_makes_the_link_unreadable_never_a_link_without_removal() {
    let r = row("Link");
    assert_eq!(
        read(&r, &text(&r, &[("blueprint", "12")])),
        fields(&[("blueprint", "12")])
    );
    assert_eq!(
        read(&r, &text(&r, &[("blueprint", "12"), ("remove", "1")])),
        fields(&[("blueprint", "12"), ("remove", "1")])
    );
    for bad in ["2", "0", "", "yes"] {
        assert_eq!(
            read(&r, &text(&r, &[("blueprint", "12"), ("remove", bad)])),
            Read::Unreadable,
            "{bad:?}"
        );
    }
}

#[test]
fn another_rows_marker_or_one_with_no_close_is_not_this_rows_marker() {
    let link = row("Link");
    let ticket_of = row("TicketOf");
    assert_eq!(
        read(&link, &text(&ticket_of, &[("ticket", "KEY-1")])),
        Read::NoMarker
    );
    let open = text(&link, &[("blueprint", "12")]);
    assert_eq!(read(&link, open.trim_end_matches("-->")), Read::NoMarker);
}

#[test]
fn another_value_outside_its_type_is_unreadable_of_the_kind() {
    assert_eq!(
        read(
            &row("Question"),
            &text(
                &row("Question"),
                &[("kind", "plan"), ("issue", "7"), ("turn", "01")]
            )
        ),
        Read::Unreadable
    );
    assert_eq!(
        read(&row("Link"), &text(&row("Link"), &[("blueprint", "012")])),
        Read::Unreadable
    );
    assert_eq!(
        read(&row("Link"), &text(&row("Link"), &[])),
        Read::Unreadable
    );
}
