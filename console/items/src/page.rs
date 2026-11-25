//! The page of an item: its title, a link to the issue on GitHub and its
//! files, images and videos shown as they are, HTML files in a sandboxed frame,
//! and the style sheets, scripts and data an HTML file refers to beside them.
//! The caller puts its own head and foot around it.
//!
//! Every value that came from GitHub is escaped, so no title, name or reason
//! can start a tag of its own.

use serde_json::Value;

/// The ticket of an item, as its `.meta.json` has it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Ticket {
    pub number: i64,
    pub title: String,
    pub url: String,
    /// Empty when no one is named.
    pub author: String,
    pub updated_at: String,
}

/// One file of an item: what the page shows, or what its HTML uses when the
/// kind is `part`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub name: String,
    pub kind: String,
    pub bytes: i64,
}

/// One file the store did not take, and why.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Skipped {
    pub name: String,
    pub reason: String,
}

/// An item's `.meta.json`, as far as its page reads it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Meta {
    pub ticket: Ticket,
    pub files: Vec<File>,
    pub skipped: Vec<Skipped>,
}

impl Meta {
    /// The meta of this text: None unless it is JSON whose `files` and
    /// `skipped` are lists. A text that is not text and a number that is no
    /// whole number of an i64 read as the page has always shown them, empty
    /// and 0: the file is the store's own, and a page of it is still a page.
    pub fn read(text: &str) -> Option<Meta> {
        let row: Value = serde_json::from_str(text).ok()?;
        let text = |row: &Value, key: &str| match row.get(key) {
            Some(Value::String(text)) => text.clone(),
            _ => String::new(),
        };
        let whole = |row: &Value, key: &str| row.get(key).and_then(Value::as_i64).unwrap_or(0);
        let files = row["files"]
            .as_array()?
            .iter()
            .map(|file| File {
                name: text(file, "name"),
                kind: text(file, "kind"),
                bytes: whole(file, "bytes"),
            })
            .collect();
        let skipped = row["skipped"]
            .as_array()?
            .iter()
            .map(|miss| Skipped {
                name: text(miss, "name"),
                reason: text(miss, "reason"),
            })
            .collect();
        Some(Meta {
            ticket: Ticket {
                number: whole(&row, "number"),
                title: text(&row, "title"),
                url: text(&row, "url"),
                author: text(&row, "author"),
                updated_at: text(&row, "updatedAt"),
            },
            files,
            skipped,
        })
    }
}

/// Text as the page holds it: every value that came from GitHub, escaped as
/// jq's @html escapes it.
pub fn h(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '&' => out.push_str("&amp;"),
            '\'' => out.push_str("&#39;"),
            '"' => out.push_str("&quot;"),
            _ => out.push(c),
        }
    }
    out
}

/// Bytes as the page says them: 32 B, 2 KB, 4.1 MB, 10 MB. The same rounding
/// and the same numbers as the jq of before: a KB is whole, an MB may carry a
/// tenth.
pub fn size(bytes: i64) -> String {
    if bytes >= 1_048_576 {
        format!("{} MB", number(((bytes as f64) / 104_857.6).round() / 10.0))
    } else if bytes >= 1024 {
        format!("{} KB", number(((bytes as f64) / 1024.0).round()))
    } else {
        format!("{bytes} B")
    }
}

/// A number as jq prints one: whole without a fraction, a tenth when the
/// value has one. 10 MB, 4.1 MB, never 10.0.
fn number(value: f64) -> String {
    if value.fract() == 0.0 {
        return format!("{}", value);
    }
    format!("{value}")
}

/// One or many, as the page words it: `4 FILES`.
pub fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The day an item changed, as the page says it: 09 OCT 2026. The month is
/// the short name of English, whatever the browser's own language; a time
/// GitHub did not answer in the shape it answers is a dash, as the jq of
/// before said.
pub fn day(updated: &str) -> String {
    const MONTHS: [&str; 12] = [
        "JAN", "FEB", "MAR", "APR", "MAY", "JUN", "JUL", "AUG", "SEP", "OCT", "NOV", "DEC",
    ];
    if !tehas_core::time::iso_shaped(updated) {
        return "-".to_string();
    }
    let month = updated[5..7].parse::<i64>().unwrap_or(0);
    let Some(month) = MONTHS.get((month - 1).try_into().unwrap_or(usize::MAX)) else {
        return "-".to_string();
    };
    format!("{} {month} {}", &updated[8..10], &updated[0..4])
}

/// A file's address under the page: jq's @uri for each part of its name, the
/// folder of a zip's file kept.
pub fn src(name: &str) -> String {
    name.split('/')
        .map(|part| {
            let mut out = String::new();
            for byte in part.as_bytes() {
                match byte {
                    b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                        out.push(*byte as char)
                    }
                    _ => out.push_str(&format!("%{byte:02X}")),
                }
            }
            out
        })
        .collect::<Vec<_>>()
        .join("/")
}

/// Ten blocks, one for each started tenth of an item's limit that is in use.
pub fn meter(bytes: i64, limit: i64) -> String {
    let on = if limit <= 0 {
        10
    } else {
        (((bytes as f64) * 10.0 / (limit as f64)).ceil() as i64).clamp(0, 10)
    };
    let blocks: String = (0..10)
        .map(|i| {
            if i < on {
                "<i class=\"on\"></i>"
            } else {
                "<i></i>"
            }
        })
        .collect();
    format!("<span class=\"item-meter\" aria-hidden=\"true\">{blocks}</span>")
}

/// The caption under a file, and the file itself: an image, a video, or HTML
/// in a frame of its own.
fn asset(name: &str, kind: &str, bytes: i64) -> String {
    let href = "files/".to_string() + &src(name);
    let shown = match kind {
        "image" => format!(
            "<a class=\"asset-view\" href=\"{href}\"><img src=\"{href}\" alt=\"Image {}, attached to the ticket\" loading=\"lazy\"></a>",
            h(name)
        ),
        "video" => format!(
            "<div class=\"asset-view\"><video src=\"{href}\" controls preload=\"metadata\" aria-label=\"Video {}, attached to the ticket\"></video></div>",
            h(name)
        ),
        _ => format!(
            "<div class=\"asset-view\"><iframe src=\"{href}\" title=\"HTML file {}\" sandbox=\"allow-scripts allow-forms allow-modals allow-pointer-lock\" loading=\"lazy\"></iframe></div>",
            h(name)
        ),
    };
    let (word, open) = match kind {
        "image" => ("IMAGE", "OPEN"),
        "video" => ("VIDEO", "OPEN"),
        _ => ("HTML", "OPEN FULL SIZE"),
    };
    format!(
        "        <figure class=\"asset{}\"><{}>{}</{}><figcaption><span class=\"asset-name\">{}</span><span class=\"asset-fact\">{}, {}</span><a href=\"{href}\" target=\"_blank\" rel=\"noopener noreferrer\" aria-label=\"Open {} by itself\">{}</a></figcaption></figure>\n",
        if kind == "app" { " asset-app" } else { "" },
        if kind == "image" { "a" } else { "div" },
        shown,
        if kind == "image" { "a" } else { "div" },
        h(name),
        word,
        size(bytes).to_ascii_uppercase(),
        h(name),
        open,
    )
}

/// One file the HTML of an item uses, in the list beside the assets.
fn part_line(file: &File) -> String {
    let name = &file.name;
    format!(
        "          <li><span>{}</span><span class=\"asset-fact\">{}</span><a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">OPEN</a></li>\n",
        h(name),
        size(file.bytes).to_ascii_uppercase(),
        "files/".to_string() + &src(name),
    )
}

/// One file the store did not take, with the reason it did not.
fn skipped_line(miss: &Skipped) -> String {
    format!(
        "          <li><span>{}</span><span class=\"asset-fact\">{}</span></li>\n",
        h(&miss.name),
        h(&miss.reason),
    )
}

/// The link to the issue, only when GitHub's own address is one.
pub fn issue_link(url: &str) -> Option<String> {
    url.starts_with("https://github.com/")
        .then(|| url.to_string())
}

/// What the files of an item come to: what shows, what the HTML uses, and the
/// bytes of both.
fn tally(files: &[File]) -> (Vec<&File>, Vec<&File>, i64) {
    let mut shown = Vec::new();
    let mut parts = Vec::new();
    let mut bytes: i64 = 0;
    for file in files {
        // The bytes come from a `.meta.json` on disk. jq added them as numbers
        // of its own and could not overflow; a run of them that does is a bad
        // figure, not a reason to stop the page, so it tops out.
        bytes = bytes.saturating_add(file.bytes);
        if file.kind == "part" {
            parts.push(file);
        } else {
            shown.push(file);
        }
    }
    (shown, parts, bytes)
}

/// The body of an item's page: the files it shows, the ones its HTML uses,
/// and the ones the store did not take. `head` is the word over its number.
pub fn item(
    ticket: &Ticket,
    files: &[File],
    skipped: &[Skipped],
    outside: i64,
    limit: i64,
    head: &str,
) -> String {
    let number = ticket.number;
    let issue = issue_link(&ticket.url);
    let button = issue.as_deref().map_or(String::new(), |url| format!("        <p><a class=\"button button-primary\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">OPEN THE TICKET ON GITHUB</a></p>\n", h(url)));
    let opened = if ticket.author.is_empty() {
        "-".to_string()
    } else {
        h(&ticket.author)
    };
    let (shown, parts, bytes) = tally(files);
    let mut out = format!(
        "    <section class=\"item-ticket\" aria-labelledby=\"ticket-title\">\n      <p class=\"item-box-head\"><span>{head}</span><b>#{number}</b></p>\n      <div class=\"item-ticket-body\">\n        <h1 id=\"ticket-title\">{}</h1>\n        <dl class=\"item-facts\">\n          <div><dt>OPENED BY</dt><dd>{opened}</dd></div>\n          <div><dt>CHANGED</dt><dd>{}</dd></div>\n        </dl>\n",
        h(&ticket.title),
        day(&ticket.updated_at),
    );
    out.push_str(&button);
    out.push_str(&format!(
        "      </div>\n    </section>\n    <section class=\"item-box\" id=\"assets\" aria-labelledby=\"assets-title\">\n      <div class=\"item-box-head\"><h2 id=\"assets-title\">ASSETS</h2><p>{}, {} OF {} {}</p></div>\n      <div class=\"item-box-body\">\n",
        plural(files.len() as i64, "FILE", "FILES"),
        size(bytes).to_ascii_uppercase(),
        size(limit).to_ascii_uppercase(),
        meter(bytes, limit),
    ));
    if files.is_empty() {
        out.push_str("        <p class=\"item-note\">No image, video or HTML file is attached to this ticket. Attach one to the issue on GitHub and it is here within a few minutes.</p>\n");
    }
    for file in &shown {
        out.push_str(&asset(&file.name, &file.kind, file.bytes));
    }
    if !parts.is_empty() {
        out.push_str(
            "        <div class=\"item-list\"><h3>FILES THE HTML USES</h3>\n        <ul>\n",
        );
        for part in &parts {
            out.push_str(&part_line(part));
        }
        out.push_str("        </ul></div>\n");
    }
    if !skipped.is_empty() || outside > 0 {
        out.push_str("        <div class=\"item-list\"><h3>LEFT OUT</h3>\n        <ul>\n");
        for miss in skipped {
            out.push_str(&skipped_line(miss));
        }
        if outside > 0 {
            out.push_str(&format!(
                "          <li><span>{} from outside the allowed accounts</span><span class=\"asset-fact\">Only files linked by an allowed account, in text nobody else edited, are taken.</span></li>\n",
                plural(outside, "link", "links"),
            ));
        }
        out.push_str("        </ul></div>\n");
    }
    out.push_str("      </div>\n    </section>\n");
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: i64 = 10 * 1024 * 1024;
    const ITEM: &str = r#"{"number":7,"title":"Map <script>alert(1)</script> & pins","url":"https://github.com/acme/widget/issues/7","author":"alice"}"#;
    const FILES: &str = r#"[{"name":"image-1.png","kind":"image","bytes":7},{"name":"proto.html","kind":"app","bytes":10},{"name":"style.css","kind":"part","bytes":4},{"name":"video-2.mp4","kind":"video","bytes":11}]"#;

    #[test]
    fn a_value_of_github_cannot_start_a_tag() {
        assert_eq!(
            h("<a href=\"x\">&'</a>"),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
        assert_eq!(h(""), "");
    }

    #[test]
    fn bytes_are_said_as_the_page_says_them() {
        assert_eq!(size(0), "0 B");
        assert_eq!(size(32), "32 B");
        assert_eq!(size(1023), "1023 B");
        assert_eq!(size(1024), "1 KB");
        assert_eq!(size(1536), "2 KB");
        assert_eq!(size(2047), "2 KB");
        assert_eq!(size(1_048_576), "1 MB");
        assert_eq!(size(4_294_967), "4.1 MB");
        assert_eq!(size(10 * 1024 * 1024), "10 MB");
        assert_eq!(size(-1), "-1 B");
    }

    #[test]
    fn a_day_as_the_page_says_it_or_a_dash() {
        assert_eq!(day("2026-10-09T08:00:00Z"), "09 OCT 2026");
        assert_eq!(day("2026-01-01T00:00:00Z"), "01 JAN 2026");
        for bad in [
            "",
            "-",
            "2026-1-9T08:00:00Z",
            "2026-10-09 08:00:00Z",
            "2026-10-09T08:00:00",
            "yesterday",
        ] {
            assert_eq!(day(bad), "-", "{bad}");
        }
    }

    #[test]
    fn a_file_is_addressed_under_the_page() {
        assert_eq!(src("shot.png"), "shot.png");
        assert_eq!(src("good/index.html"), "good/index.html");
        assert_eq!(src("2F..-2Fevil.html"), "2F..-2Fevil.html");
        assert_eq!(src("a b.c"), "a%20b.c");
        assert_eq!(src("good/da ta/s.js"), "good/da%20ta/s.js");
    }

    #[test]
    fn the_meter_fills_one_block_for_each_started_tenth() {
        assert_eq!(meter(0, LIMIT).matches("class=\"on\"").count(), 0);
        assert_eq!(meter(1, LIMIT).matches("class=\"on\"").count(), 1);
        assert_eq!(meter(5_242_880, LIMIT).matches("class=\"on\"").count(), 5);
        assert_eq!(meter(LIMIT, LIMIT).matches("class=\"on\"").count(), 10);
        assert_eq!(meter(2 * LIMIT, LIMIT).matches("class=\"on\"").count(), 10);
        assert_eq!(meter(0, 0).matches("class=\"on\"").count(), 10);
    }

    #[test]
    fn only_githubs_own_address_is_taken_as_the_ticket() {
        assert_eq!(
            issue_link("https://github.com/acme/widget/issues/7"),
            Some("https://github.com/acme/widget/issues/7".to_string())
        );
        assert_eq!(issue_link("https://example.com/7"), None);
        assert_eq!(issue_link(""), None);
    }

    #[test]
    fn a_file_shown_is_not_a_file_the_html_uses() {
        let files = meta(ITEM, FILES, "[]").files;
        let (shown, parts, bytes) = tally(&files);
        assert_eq!(shown.len(), 3);
        assert_eq!(parts.len(), 1);
        assert_eq!(bytes, 32);
        assert_eq!(tally(&[]), (Vec::new(), Vec::new(), 0));
    }

    #[test]
    fn bytes_that_would_overflow_a_page_top_out_instead_of_stopping_it() {
        // The figure of a `.meta.json` is what bash wrote, so it is small. One
        // that is not is a bad file, and jq - which added these before - could
        // not overflow: a page is printed with the largest count it can hold
        // and the pass goes on to the next item.
        let big = meta(
            r#"{"number":7,"title":"Big","url":"u"}"#,
            r#"[{"name":"a","kind":"image","bytes":9223372036854775807},{"name":"b","kind":"image","bytes":9223372036854775807}]"#,
            "[]",
        );
        let huge = &big.files;
        let (shown, parts, bytes) = tally(huge);
        assert_eq!((shown.len(), parts.len()), (2, 0));
        assert_eq!(bytes, i64::MAX);
        assert_eq!(size(bytes), "8796093022208 MB");
        // A page of them is a page: it says its bytes and holds both files.
        let page = item(&big.ticket, huge, &[], 0, LIMIT, "ITEM");
        assert!(page.contains("2 FILES, 8796093022208 MB OF 10 MB"));
        assert!(page.contains("<figcaption><span class=\"asset-name\">a</span>"));
        assert!(page.contains("<figcaption><span class=\"asset-name\">b</span>"));
    }

    #[test]
    fn an_items_page_holds_its_title_its_link_and_its_files() {
        let m = meta(
            ITEM,
            FILES,
            r#"[{"name":"all.tar","reason":"Not a type the host serves."}]"#,
        );
        let page = item(&m.ticket, &m.files, &m.skipped, 1, LIMIT, "ITEM");
        assert!(page.contains("<span>ITEM</span><b>#7</b>"));
        assert!(page.contains("Map &lt;script&gt;alert(1)&lt;/script&gt; &amp; pins"));
        assert!(!page.contains("<script"));
        assert!(page.contains("href=\"https://github.com/acme/widget/issues/7\""));
        assert!(page.contains("<img src=\"files/image-1.png\""));
        assert!(page.contains("<video src=\"files/video-2.mp4\""));
        assert!(page.contains("<iframe src=\"files/proto.html\""));
        assert!(
            page.contains("sandbox=\"allow-scripts allow-forms allow-modals allow-pointer-lock\"")
        );
        assert!(page.contains("<span>style.css</span>"));
        assert!(page.contains("4 FILES, 32 B OF 10 MB"));
        assert!(page.contains(
            "<span>all.tar</span><span class=\"asset-fact\">Not a type the host serves.</span>"
        ));
        assert!(page.contains("1 link from outside the allowed accounts</span>"));
        assert!(page.contains("<figcaption><span class=\"asset-name\">image-1.png</span>"));
        assert!(page.contains("aria-label=\"Open proto.html by itself\">OPEN FULL SIZE</a>"));
    }

    /// The meta of a ticket's object with these lists.
    fn meta(ticket: &str, files: &str, skipped: &str) -> Meta {
        let open = ticket.trim_end().strip_suffix('}').unwrap();
        Meta::read(&format!("{open},\"files\":{files},\"skipped\":{skipped}}}")).unwrap()
    }

    // A meta is read whole or not at all, and a value of a kind the store
    // does not write reads as nothing.
    #[test]
    fn a_meta_is_read_into_the_ticket_the_files_and_what_was_left_out() {
        let m = meta(ITEM, FILES, r#"[{"name":"all.tar","reason":"Too big."}]"#);
        assert_eq!(
            m.ticket,
            Ticket {
                number: 7,
                title: "Map <script>alert(1)</script> & pins".into(),
                url: "https://github.com/acme/widget/issues/7".into(),
                author: "alice".into(),
                updated_at: String::new(),
            }
        );
        assert_eq!(m.files.len(), 4);
        assert_eq!(
            m.files[2],
            File {
                name: "style.css".into(),
                kind: "part".into(),
                bytes: 4
            }
        );
        assert_eq!(
            m.skipped,
            [Skipped {
                name: "all.tar".into(),
                reason: "Too big.".into()
            }]
        );
        let odd =
            Meta::read(r#"{"number":"7","title":5,"files":[3,{"bytes":1.5}],"skipped":[null]}"#)
                .unwrap();
        assert_eq!((odd.ticket.number, odd.ticket.title.as_str()), (0, ""));
        assert_eq!(odd.files[0], odd.files[1]);
        assert_eq!(odd.files[0].bytes, 0);
        assert_eq!(odd.skipped[0].reason, "");
        for none in [
            "",
            "not json",
            "[]",
            r#"{"files":[]}"#,
            r#"{"skipped":[]}"#,
            r#"{"files":{},"skipped":[]}"#,
            r#"{"files":[],"skipped":"none"}"#,
        ] {
            assert_eq!(Meta::read(none), None, "{none}");
        }
    }

    /// The page of a `.meta.json`, drawn as the store draws it.
    fn page_of(meta: &str, outside: i64) -> String {
        let m = Meta::read(meta).unwrap();
        item(&m.ticket, &m.files, &m.skipped, outside, LIMIT, "ITEM")
    }

    // The whole page, byte for byte: of a meta as the store writes one, and
    // of one whose values are of no kind the store writes, which the page
    // reads as nothing rather than refusing.
    #[test]
    fn the_page_of_a_meta_is_these_bytes() {
        assert_eq!(
            page_of(include_str!("testdata/meta-plain.json"), 2),
            include_str!("testdata/page-plain.html")
        );
        assert_eq!(
            page_of(include_str!("testdata/meta-odd.json"), 0),
            include_str!("testdata/page-odd.html")
        );
    }

    #[test]
    fn an_item_with_nothing_attached_says_so() {
        let page = item(&meta(ITEM, "[]", "[]").ticket, &[], &[], 0, LIMIT, "ITEM");
        assert!(page.contains("No image, video or HTML file is attached"));
        assert!(page.contains("0 FILES, 0 B OF 10 MB"));
        assert!(!page.contains("LEFT OUT"));
        assert!(!page.contains("FILES THE HTML USES"));
    }

    #[test]
    fn an_item_no_one_opened_has_a_dash_for_its_author() {
        for author in ["null", "\"\"", "3"] {
            let row = ITEM.replace("\"alice\"", author);
            let ticket = meta(&row, "[]", "[]").ticket;
            assert!(item(&ticket, &[], &[], 0, LIMIT, "ITEM").contains("<dd>-</dd>"));
        }
    }
}
