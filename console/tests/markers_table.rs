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
    "Spec",
    "LastAttempt",
    "Millwright",
    "TicketOf",
    "Link",
];

/// The kinds that are a block: an open line, a payload, a close line.
const BLOCKS: [&str; 2] = ["Spec", "LastAttempt"];

/// The field types the table's header defines, each without its arguments.
const TYPES: [&str; 10] = [
    "blueprint",
    "number",
    "numbers",
    "words",
    "word",
    "hex",
    "key",
    "link",
    "requirements",
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
                    ["link", "hex", "requirements", "text"].contains(&base),
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
