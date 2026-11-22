//! The Protolab: a page for every ticket at the prototype stage, with the
//! files attached to the ticket beside it, written to $TEHAS_HOME/items,
//! which the edge router serves on port 8081 (edge/router.compose.yaml): by
//! path, and with TEHAS_DOMAIN set, each ticket also under a name of its own,
//! item-<number>.<domain>.
//!
//!   console protolab run
//!
//! `tehasbot protolab` loads the settings and the GitHub App's token
//! (src/run.rs) and runs this under them: one line per ticket built or removed;
//! exit 0 even when GitHub is down, what is there is then left in place; 78 the
//! instance settings are invalid; 1 it cannot write. A pass is in
//! protolab/build.rs. Which tickets and which links are read by the issue feed
//! (tehas_core::issue_feed), and the files are kept and the page of a ticket
//! written by tehas_items. This file holds the lab's own words: the head and
//! foot of a page, the lab's front page and live/protolab.json.
//!
//! A ticket is at the prototype stage while it is open and carries
//! tehas:prototype applied by an account the instance allows. Its page has the
//! title, a link to the issue on GitHub and the attached files: images and
//! videos shown as they are, HTML files shown in a frame, and the style sheets,
//! scripts and data an HTML file refers to beside them. Everything is static.
//! Nothing here runs a prototype.
//!
//!   which tickets   Open issues whose most recent tehas:prototype labelling is
//!                   by an account in TEHAS_ALLOWED_LABELERS. One applied by
//!                   anyone else, the bot included, counts as no label at all:
//!                   the ticket gets no page, and a page it had is removed. The
//!                   log names the issue and the account.
//!   which files     Only links to GitHub's own attachment storage, in the issue
//!                   text and its comments, written by an account in
//!                   TEHAS_ALLOWED_LABELERS and last edited by none outside it,
//!                   on a line that is no quote. Nothing else is fetched,
//!                   save a zip named by address and sha256 below.
//!   a zip by address  With protolab-zip-url on (TEHAS_PROTOLAB_ZIP_URL, still
//!                   being built), a line `Zip: <link> sha256:<64 hex>` in such
//!                   a text names a zip: a file attached on GitHub, or an https
//!                   address on a host of TEHAS_PROTOLAB_ZIP_HOSTS, followed
//!                   only to addresses on those hosts. Bytes of another sha256
//!                   are refused. It is unpacked as an attached zip is, under
//!                   the same limits, after the attached files.
//!   which types     png jpg gif webp svg avif, mp4 webm mov, html htm, and
//!                   css js mjs json txt. A zip is unpacked into a folder of
//!                   its own by libexec/attach, which takes the same types and
//!                   md; its files count against the same limits.
//!   how much        10 megabytes a ticket, counted in the order the files were
//!                   attached, and at most 40 files. What does not fit is left
//!                   out and the page says so.
//!   when it goes    The page and its files are removed when the ticket is
//!                   closed, loses the label, or carries it from an account the
//!                   instance does not allow. GitHub keeps the originals.
//!
//! Who applied a label is read by `tehas_core::forge`, the same reading the
//! dispatcher uses for tehas:ready: authorisation is the account, not the label
//! (AGENTS.md rule 5). Where the events of a ticket cannot be read at all,
//! nothing is built and nothing is removed for that ticket; a page it had stays
//! as it was. An instance that lists the bot's own login in
//! TEHAS_ALLOWED_LABELERS is refused: an account in a cycle labels as the bot,
//! so such a list would let an agent choose what this publishes. A pass that
//! cannot read its own login ends there, with nothing built and nothing removed.
//!
//! A ticket whose issue has not changed since the last run is left alone, so a
//! run with nothing new fetches no files: it makes one GitHub call for its own
//! account, one for the list, and one for every ticket at the stage, its label
//! events. A ticket that changed has its texts read; when the title and the
//! links taken are the ones its page was built from, the page only takes the
//! new time and no file is fetched again. A console with other bytes builds
//! every page again. No model.
//!
//! It also writes $TEHAS_HOME/live/protolab.json, the list the dashboard links
//! from. TEHAS_PROTOLAB_URL in instance.env is the address people reach the
//! Protolab at, when that is not port 8081 of the dashboard's own host.
//!
//! Every value that came from GitHub is escaped, so no title, name or reason
//! can start a tag of its own.
//!
//! Exit: 0 even when GitHub is down, what is there is then left in place; 1 it
//! cannot write; 64 usage; 78 the settings are missing, or the bot is on the
//! allowed list. One line per ticket built or removed on standard output.
//!
//! The shape of every page here is what test/protolab_test.sh pins, line for
//! line, so that the pages of a host of this tehas are the pages of the host
//! before it.

mod build;

use serde_json::{Map, Value};
use tehas_items::page::{self, bytes_of, h, issue_link, number_of, plural, size, text_of};

pub fn main(given: &[String]) -> u8 {
    match given {
        [verb] if verb == "run" => build::run(),
        _ => {
            eprintln!("usage: console protolab run");
            64
        }
    }
}

// --- The page ---------------------------------------------------------------
// Every line of markup here is the line the pages of the lab have always had.

const ASCII: &str = r"       __________________________.____    _________    ______
       \___     ___/  __    /    |    \ _/    _    \ _/   __/__
__ __/\ _/       \_   \____/__   _     \__   _/     \\______   \_ /\___ ___ _
       \____     /____      /___|      //___\       /      ____/
 - --------l____/----l_____/----l_____/------\_____/_______/-----sQ!-- -- -";

/// The door of a ticket: a name of its own with a domain, its number as a
/// path without one. `domain` is what follows the label,
/// [`tehas_core::settings::edge_suffix`]: `.example.org`, or `-demo.example.org`
/// one level deep.
fn door(number: i64, domain: &str) -> String {
    if domain.is_empty() {
        format!("/{number}/")
    } else {
        format!("https://item-{number}{domain}/")
    }
}

/// The theme file and the logo of a design, as TEHAS_DESIGN names it: the
/// pixel factory for pixel, the logo for beesknees26 and anything else.
fn look(design: &str) -> (&'static str, &'static str) {
    if design == "pixel" {
        ("theme-pixel.css", "factory.svg\" width=\"80\" height=\"50")
    } else {
        (
            "theme-beesknees26.css",
            "favicon.svg\" width=\"50\" height=\"50",
        )
    }
}

/// The head of a page: the style sheets of host/fun, the wordmark, the menu.
fn top(title: &str, menu: &str, home: &str, design: &str) -> String {
    let home = h(&Value::from(home));
    let (theme, logo) = look(design);
    let mut out = String::from("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str(&format!("  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <meta name=\"robots\" content=\"noindex, nofollow\">\n  <title>{} - Protolab</title>\n", h(&Value::from(title))));
    out.push_str(&format!("  <link rel=\"icon\" type=\"image/svg+xml\" href=\"/fun/favicon.svg\">\n  <link rel=\"stylesheet\" href=\"/fun/{theme}\">\n  <link rel=\"stylesheet\" href=\"/fun/game-view.css\">\n  <link rel=\"stylesheet\" href=\"/fun/protolab.css\">\n"));
    out.push_str("</head>\n<body id=\"top\">\n  <a class=\"skip\" href=\"#page\">Skip to the page</a>\n  <header class=\"topbar topbar-short\">\n");
    out.push_str(&format!("    <a class=\"brand\" href=\"{home}\" aria-label=\"Protolab, every prototype\">\n      <img src=\"/fun/{logo}\" alt=\"\">\n      <span>PROTOLAB</span>\n    </a>\n    <nav class=\"menu\" aria-label=\"Site\">\n      <a href=\"{home}\">ALL PROTOTYPES</a>\n"));
    out.push_str(menu);
    out.push_str("    </nav>\n  </header>\n  <main class=\"lab-stage\" id=\"page\">\n");
    out
}

/// The foot of a page: what the lab is, and the art and the credits under it.
fn bottom(limit: i64) -> String {
    format!(
        "  </main>\n  <footer class=\"footer\">\n    <p class=\"footer-note\">The Protolab holds tickets at the prototype stage: open issues labelled tehas:prototype by an allowed account. The files are copies of what is attached to the issue, at most {} a ticket. This view is read-only; a page goes when its ticket is closed or loses the label.</p>\n    <div class=\"ascii\"><pre role=\"img\" aria-label=\"Tehas ASCII art logo\">{}</pre></div>\n    <p class=\"credit\">ASCII ART: <a href=\"https://demozoo.org/sceners/26586/\" target=\"_blank\" rel=\"noopener noreferrer\">SQUAREPUSHER / TEHDAS</a>. PIXEL TYPE: <a href=\"https://github.com/rewtnull/amigafonts\" target=\"_blank\" rel=\"noopener noreferrer\">TOPAZPLUS A1200</a>, TRUETYPE VERSION BY DMG OF TRUESCHOOL AND DIVINE STYLERS. TOPAZ IS COPYRIGHT AMIGAINC.</p>\n  </footer>\n</body>\n</html>",
        size(limit).to_ascii_uppercase(),
        h(&Value::from(ASCII)),
    )
}

/// The page of one ticket: its menu and the lab's head and foot around the
/// page of the item. The ticket's own address is a name with a domain and a
/// path without one, which is what the menu and the row of the list use.
fn ticket_page(
    row: &Value,
    files: &[Value],
    skipped: &[Value],
    outside: i64,
    home: &str,
    design: &str,
    limit: i64,
) -> String {
    let number = number_of(row);
    let title = text_of(row, "title");
    let menu = issue_link(&text_of(row, "url"))
        .map_or(String::new(), |url| format!("      <a class=\"repo\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">TICKET ON GITHUB</a>\n", h(&Value::from(url))));
    let mut out = top(&format!("#{number} {title}"), &menu, home, design);
    out.push_str(&page::item(
        row,
        files,
        skipped,
        outside,
        limit,
        "PROTOTYPE",
    ));
    out.push_str(&bottom(limit));
    out
}

/// One row of the lab's list.
fn row_of(item: &Value, domain: &str) -> String {
    let number = number_of(item);
    let files = item["files"].as_array().map_or(0, Vec::len) as i64;
    let stage = if files == 0 {
        "NO FILES".to_string()
    } else {
        format!(
            "{}, {}",
            plural(files, "FILE", "FILES"),
            size(bytes_of(item)).to_ascii_uppercase()
        )
    };
    format!(
        "          <li><a class=\"row\" href=\"{}\"><b>#{number}</b><span class=\"row-title\">{}</span><span class=\"row-stage\">{}</span></a></li>\n",
        h(&Value::from(door(number, domain).as_str())),
        h(&Value::from(text_of(item, "title").as_str())),
        stage,
    )
}

/// The lab's front page.
fn front_page(tickets: &[Value], domain: &str, home: &str, design: &str, limit: i64) -> String {
    let mut out = top("Prototypes", "", home, design);
    out.push_str(&format!(
        "    <section class=\"lab\" aria-labelledby=\"lab-title\">\n      <div class=\"lab-head\"><h1 id=\"lab-title\">PROTOLAB</h1><p>{}</p></div>\n      <div class=\"lab-body\">\n        <ul class=\"rows\">\n",
        plural(tickets.len() as i64, "PROTOTYPE", "PROTOTYPES"),
    ));
    if tickets.is_empty() {
        out.push_str("          <li class=\"rows-note\">Nothing in the lab. An allowed account labels an open ticket tehas:prototype to put it here.</li>\n");
    } else {
        for item in tickets {
            out.push_str(&row_of(item, domain));
        }
    }
    out.push_str("        </ul>\n        <figure class=\"lab-scene\">\n          <img src=\"/fun/scene-lab.svg\" width=\"400\" height=\"280\" alt=\"Through an office window: one colleague has an idea, a second sketches it in a notebook, and a third reads it and passes it on.\">\n          <figcaption>Each ticket has a page with its title, a link to the issue and the images, videos and HTML files attached to it.</figcaption>\n        </figure>\n      </div>\n    </section>\n");
    out.push_str(&bottom(limit));
    out
}

/// live/protolab.json: what the dashboard links from, and nothing else.
fn live_file(tickets: &[Value], now: &str, url: &str, domain: &str, limit: i64) -> String {
    let rows: Vec<Value> = tickets
        .iter()
        .map(|item| {
            let mut row = Map::new();
            row.insert("number".into(), Value::from(number_of(item)));
            row.insert("title".into(), Value::from(text_of(item, "title")));
            row.insert("url".into(), Value::from(text_of(item, "url")));
            row.insert(
                "files".into(),
                Value::from(item["files"].as_array().map_or(0, Vec::len) as i64),
            );
            row.insert("bytes".into(), Value::from(bytes_of(item)));
            if let Some(built) = item["builtAt"].as_str() {
                row.insert("builtAt".into(), Value::from(built));
            }
            row.insert(
                "address".into(),
                if domain.is_empty() {
                    Value::Null
                } else {
                    Value::from(door(number_of(item), domain))
                },
            );
            Value::Object(row)
        })
        .collect();
    let mut root = Map::new();
    root.insert("generatedAt".into(), Value::from(now));
    root.insert("url".into(), Value::from(url));
    root.insert("limitBytes".into(), Value::from(limit));
    root.insert("tickets".into(), Value::Array(rows));
    format!("{}\n", Value::Object(root))
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: i64 = 10 * 1024 * 1024;

    fn value_of(text: &str) -> Value {
        serde_json::from_str(text).unwrap_or(Value::Null)
    }

    /// One JSON value a line, or one list.
    fn lines(text: &str) -> Vec<Value> {
        if let Value::Array(rows) = value_of(text) {
            return rows;
        }
        text.lines().map(value_of).collect()
    }

    fn ticket() -> Value {
        value_of(
            r#"{"number":7,"title":"Map <script>alert(1)</script> & pins","url":"https://github.com/acme/widget/issues/7","author":"alice","updatedAt":"2026-10-09T08:00:00Z"}"#,
        )
    }

    fn files() -> Vec<Value> {
        lines(
            "{\"name\":\"image-1.png\",\"kind\":\"image\",\"bytes\":7}\n{\"name\":\"proto.html\",\"kind\":\"app\",\"bytes\":10}\n{\"name\":\"style.css\",\"kind\":\"part\",\"bytes\":4}\n{\"name\":\"video-2.mp4\",\"kind\":\"video\",\"bytes\":11}\n",
        )
    }

    #[test]
    fn a_ticket_is_linked_by_name_only_with_a_domain() {
        assert_eq!(door(7, ""), "/7/");
        assert_eq!(door(7, ".example.org"), "https://item-7.example.org/");
        assert_eq!(
            door(7, "-demo.example.org"),
            "https://item-7-demo.example.org/"
        );
    }

    #[test]
    fn a_tickets_page_holds_its_title_its_link_and_its_files() {
        let page = ticket_page(
            &ticket(),
            &files(),
            &lines("{\"name\":\"all.tar\",\"reason\":\"Not a type the lab hosts.\"}\n"),
            1,
            "/",
            "",
            LIMIT,
        );
        assert!(page.starts_with("<!doctype html>\n<html lang=\"en\">\n<head>\n"));
        assert!(page.contains(
            "<title>#7 Map &lt;script&gt;alert(1)&lt;/script&gt; &amp; pins - Protolab</title>"
        ));
        assert!(page.contains("<a href=\"/\">ALL PROTOTYPES</a>"));
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
            "<span>all.tar</span><span class=\"asset-fact\">Not a type the lab hosts.</span>"
        ));
        assert!(page.contains("1 link from outside the allowed accounts</span>"));
        assert!(page.contains("<figcaption><span class=\"asset-name\">image-1.png</span>"));
        assert!(page.contains("aria-label=\"Open proto.html by itself\">OPEN FULL SIZE</a>"));
        assert!(page.contains("09 OCT 2026"));
        assert!(page.ends_with("</body>\n</html>"));
    }

    #[test]
    fn a_page_is_drawn_in_the_instances_design() {
        for (design, theme, logo) in [
            (
                "",
                "theme-beesknees26.css",
                "favicon.svg\" width=\"50\" height=\"50\"",
            ),
            (
                "beesknees26",
                "theme-beesknees26.css",
                "favicon.svg\" width=\"50\" height=\"50\"",
            ),
            (
                "pixel",
                "theme-pixel.css",
                "factory.svg\" width=\"80\" height=\"50\"",
            ),
        ] {
            for page in [
                ticket_page(&ticket(), &[], &[], 0, "/", design, LIMIT),
                front_page(&[], "", "/", design, LIMIT),
            ] {
                assert!(page.contains(&format!("<link rel=\"stylesheet\" href=\"/fun/{theme}\">")));
                assert!(page.contains(&format!("<img src=\"/fun/{logo} alt=\"\">")));
                assert_eq!(
                    page.matches("rel=\"stylesheet\" href=\"/fun/theme-")
                        .count(),
                    1
                );
            }
        }
    }

    #[test]
    fn a_ticket_with_no_github_address_holds_no_link_to_github() {
        let mut row = ticket();
        row["url"] = Value::from("https://elsewhere.example/7");
        let page = ticket_page(&row, &[], &[], 0, "/", "", LIMIT);
        assert!(!page.contains("TICKET ON GITHUB"));
        assert!(!page.contains("OPEN THE TICKET"));
    }

    #[test]
    fn the_list_names_every_ticket_and_says_when_there_is_none() {
        let one = value_of(
            r#"{"number":7,"title":"Map","url":"u","files":[{"name":"a","kind":"image","bytes":3}],"bytes":3}"#,
        );
        let page = front_page(&[one], "", "/", "", LIMIT);
        assert!(page.contains("<a class=\"row\" href=\"/7/\"><b>#7</b>"));
        assert!(page.contains("1 PROTOTYPE<"));
        assert!(page.contains("1 FILE, 3 B"));
        let none = front_page(&[], "", "/", "", LIMIT);
        assert!(none.contains("Nothing in the lab."));
        assert!(none.contains("0 PROTOTYPES<"));
        let named = front_page(
            &[value_of(
                r#"{"number":7,"title":"M","url":"u","files":[],"bytes":0}"#,
            )],
            ".example.org",
            "https://lab.example.org/",
            "",
            LIMIT,
        );
        assert!(named.contains("href=\"https://item-7.example.org/\""));
        assert!(!named.contains("href=\"/7/\""));
        assert!(named.contains("<a href=\"https://lab.example.org/\">ALL PROTOTYPES</a>"));
        assert!(named.contains("NO FILES"));
    }

    #[test]
    fn the_dashboards_file_lists_the_tickets_it_was_given() {
        let tickets = lines(
            r#"{"number":7,"title":"Map","url":"https://github.com/acme/widget/issues/7","files":[{"name":"a","bytes":7},{"name":"b","bytes":25}],"bytes":32,"skipped":[],"outside":0}"#,
        );
        let said = live_file(&tickets, "2026-10-28T09:00:00Z", "", "", LIMIT);
        let read: Value = serde_json::from_str(&said).unwrap();
        assert_eq!(
            read["tickets"],
            serde_json::json!([{"number": 7, "title": "Map", "url": "https://github.com/acme/widget/issues/7", "files": 2, "bytes": 32, "address": null}])
        );
        assert_eq!(said, format!("{read}\n").replace(' ', ""));
        let named = live_file(
            &tickets,
            "2026-10-28T09:00:00Z",
            "https://lab.example",
            ".example.org",
            LIMIT,
        );
        let read: Value = serde_json::from_str(&named).unwrap();
        assert_eq!(read["url"], "https://lab.example");
        assert_eq!(read["limitBytes"], LIMIT);
        assert_eq!(read["generatedAt"], "2026-10-28T09:00:00Z");
        assert_eq!(read["tickets"][0]["address"], "https://item-7.example.org/");
        // When the page was built, as the pass read it off the page.
        let built = lines(
            r#"{"number":7,"title":"Map","url":"u","files":[],"bytes":0,"builtAt":"2026-10-28T08:00:00Z"}"#,
        );
        let read: Value =
            serde_json::from_str(&live_file(&built, "2026-10-28T09:00:00Z", "", "", LIMIT))
                .unwrap();
        assert_eq!(read["tickets"][0]["builtAt"], "2026-10-28T08:00:00Z");
    }
}
