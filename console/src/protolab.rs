//! The Protolab: a page for every ticket at the prototype stage, with the
//! files attached to the ticket beside it, written to $TEHAS_HOME/protolab,
//! which the edge router serves on port 8081 (edge/router.compose.yaml): by
//! path, and with TEHAS_DOMAIN set, each ticket also under a name of its own,
//! lab-<number>.<domain>.
//!
//!   console protolab run
//!
//! `tehasbot protolab` loads the settings and the GitHub App's token
//! (src/run.rs) and runs this under them: one line per ticket built or removed;
//! exit 0 even when GitHub is down, what is there is then left in place; 78 the
//! instance settings are invalid; 1 it cannot write. A pass is in
//! protolab/build.rs. This file holds what is
//! said: the page of a ticket, the lab's front page, live/protolab.json, and
//! the attachment links of a ticket.
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

// The links of a ticket: only GitHub's own attachment storage, by the one rule.
use attach::link::links;
use serde_json::{Map, Value};

pub fn main(given: &[String]) -> u8 {
    match given {
        [verb] if verb == "run" => build::run(),
        _ => {
            eprintln!("usage: console protolab run");
            64
        }
    }
}

/// The links of a ticket to fetch, the zips it names by address, and how many
/// came from outside.
struct Refs {
    fetch: Vec<String>,
    zips: Vec<ZipMark>,
    outside: i64,
}

/// What a `Zip:` line says: the zip's link and sha256, or, for a line of
/// another form, its first word.
type ZipMark = Result<Zip, String>;

#[derive(Clone, Debug, PartialEq, Hash)]
struct Zip {
    link: String,
    sha256: String,
}

/// The word that opens a line naming a zip by address.
const ZIP_MARK: &str = "Zip:";

/// The zip a line names: `Zip: <link> sha256:<64 lower-case hex>`. None when
/// the line does not start with [`ZIP_MARK`].
fn zip_mark(line: &str) -> Option<ZipMark> {
    let rest = line.trim().strip_prefix(ZIP_MARK)?;
    let words: Vec<&str> = rest.split_whitespace().collect();
    let hex = |h: &str| {
        h.len() == 64
            && h.bytes()
                .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))
    };
    Some(match words[..] {
        [link, sum] if sum.strip_prefix("sha256:").is_some_and(hex) => Ok(Zip {
            link: link.to_string(),
            sha256: sum["sha256:".len()..].to_string(),
        }),
        _ => Err(words.first().map_or_else(String::new, |w| w.to_string())),
    })
}

impl Refs {
    /// What a page is built from besides the files themselves: the title, the
    /// links taken and the count of the rest. The same key needs no fetch.
    fn key(&self, title: &str) -> String {
        let mut hasher = std::collections::hash_map::DefaultHasher::new();
        std::hash::Hash::hash(&(title, &self.fetch, self.outside), &mut hasher);
        if !self.zips.is_empty() {
            std::hash::Hash::hash(&self.zips, &mut hasher);
        }
        format!("{:016x}", std::hash::Hasher::finish(&hasher))
    }
}

/// The attachment links of one ticket, from the issue as the texts read
/// answers it: its body and its comments, each with its author and its last
/// editor, the logins vouched. A link is fetched when an allowed account wrote
/// the text, nobody off the list edited it last, and the line is no quote: a
/// quote reply carries someone else's words. Any other link is only counted.
/// With `zips`, a `Zip:` line is read by the same rule and its link is no
/// attachment link.
fn refs(issue: &Value, allowed: &dyn Fn(&str) -> bool, zips: bool) -> Refs {
    let mut texts = vec![issue];
    if let Some(nodes) = issue.pointer("/comments/nodes").and_then(Value::as_array) {
        texts.extend(nodes);
    }
    let mut mine = Vec::new();
    let mut named: Vec<ZipMark> = Vec::new();
    let mut theirs = Vec::new();
    for text in texts {
        let ours = allowed(&login_of(text, "author"))
            && match text.get("editor") {
                None | Some(Value::Null) => true,
                Some(_) => allowed(&login_of(text, "editor")),
            };
        for line in text_of(text, "body").lines() {
            let quoted = line.trim_start().starts_with('>');
            let unquoted = line.trim_start_matches(|c: char| c == '>' || c.is_whitespace());
            if let Some(mark) = zip_mark(unquoted).filter(|_| zips) {
                if ours && !quoted {
                    if !named.contains(&mark) {
                        named.push(mark);
                    }
                } else {
                    let link = mark.map_or_else(|word| word, |zip| zip.link);
                    if !theirs.contains(&link) {
                        theirs.push(link);
                    }
                }
                continue;
            }
            for link in links(line) {
                let keep = if ours && !quoted {
                    &mut mine
                } else {
                    &mut theirs
                };
                if !keep.contains(&link) {
                    keep.push(link);
                }
            }
        }
    }
    Refs {
        fetch: mine,
        zips: named,
        outside: theirs.len() as i64,
    }
}

/// The login of an account a text names, empty when it names none.
fn login_of(text: &Value, key: &str) -> String {
    text.get(key)
        .map_or_else(String::new, |who| text_of(who, "login"))
}

// --- The page ---------------------------------------------------------------
// Every line of markup here is the line the pages of the lab have always had.

const ASCII: &str = r"       __________________________.____    _________    ______
       \___     ___/  __    /    |    \ _/    _    \ _/   __/__
__ __/\ _/       \_   \____/__   _     \__   _/     \\______   \_ /\___ ___ _
       \____     /____      /___|      //___\       /      ____/
 - --------l____/----l_____/----l_____/------\_____/_______/-----sQ!-- -- -";

/// Text as the page holds it: every value that came from GitHub, escaped as
/// jq's @html escapes it.
fn h(value: &Value) -> String {
    let text = match value {
        Value::String(text) => text.clone(),
        Value::Null => String::new(),
        Value::Bool(on) => on.to_string(),
        Value::Number(n) => n.to_string(),
        other => other.to_string(),
    };
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
fn size(bytes: i64) -> String {
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
fn plural(count: i64, one: &str, many: &str) -> String {
    format!("{count} {}", if count == 1 { one } else { many })
}

/// The day a ticket changed, as the page says it: 09 OCT 2026. The month is
/// the short name of English, whatever the browser's own language; a time
/// GitHub did not answer in the shape it answers is a dash, as the jq of
/// before said.
fn day(updated: &str) -> String {
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
fn src(name: &str) -> String {
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

/// The door of a ticket: a name of its own with a domain, its number as a
/// path without one.
fn door(number: i64, domain: &str) -> String {
    if domain.is_empty() {
        format!("/{number}/")
    } else {
        format!("https://lab-{number}.{domain}/")
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

/// Ten blocks, one for each started tenth of a ticket's limit that is in use.
fn meter(bytes: i64, limit: i64) -> String {
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
    format!("<span class=\"lab-meter\" aria-hidden=\"true\">{blocks}</span>")
}

fn text_of(row: &Value, key: &str) -> String {
    match row.get(key) {
        Some(Value::String(text)) => text.clone(),
        _ => String::new(),
    }
}

fn bytes_of(row: &Value) -> i64 {
    row.get("bytes").and_then(Value::as_i64).unwrap_or(0)
}

fn number_of(row: &Value) -> i64 {
    row.get("number").and_then(Value::as_i64).unwrap_or(0)
}

fn count_of(row: &Value) -> i64 {
    row.get("files")
        .and_then(Value::as_array)
        .map_or(0, |rows| rows.len()) as i64
}

/// The caption under a file, and the file itself: an image, a video, or HTML
/// in a frame of its own.
fn asset(name: &str, kind: &str, bytes: i64) -> String {
    let href = "files/".to_string() + &src(name);
    let shown = match kind {
        "image" => format!(
            "<a class=\"asset-view\" href=\"{href}\"><img src=\"{href}\" alt=\"Image {}, attached to the ticket\" loading=\"lazy\"></a>",
            h(&Value::from(name))
        ),
        "video" => format!(
            "<div class=\"asset-view\"><video src=\"{href}\" controls preload=\"metadata\" aria-label=\"Video {}, attached to the ticket\"></video></div>",
            h(&Value::from(name))
        ),
        _ => format!(
            "<div class=\"asset-view\"><iframe src=\"{href}\" title=\"HTML prototype {}\" sandbox=\"allow-scripts allow-forms allow-modals allow-pointer-lock\" loading=\"lazy\"></iframe></div>",
            h(&Value::from(name))
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
        h(&Value::from(name)),
        word,
        size(bytes).to_ascii_uppercase(),
        h(&Value::from(name)),
        open,
    )
}

/// One file the HTML of the prototype uses, in the list beside the assets.
fn part_line(row: &Value) -> String {
    let name = text_of(row, "name");
    format!(
        "          <li><span>{}</span><span class=\"asset-fact\">{}</span><a href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">OPEN</a></li>\n",
        h(&Value::from(name.as_str())),
        size(bytes_of(row)).to_ascii_uppercase(),
        "files/".to_string() + &src(&name),
    )
}

/// One file the lab did not take, with the reason it did not.
fn skipped_line(row: &Value) -> String {
    format!(
        "          <li><span>{}</span><span class=\"asset-fact\">{}</span></li>\n",
        h(&Value::from(text_of(row, "name").as_str())),
        h(&Value::from(text_of(row, "reason").as_str())),
    )
}

/// The link to the issue, only when GitHub's own address is one.
fn issue_link(url: &str) -> Option<String> {
    url.starts_with("https://github.com/")
        .then(|| url.to_string())
}

/// What the files of a ticket come to: what shows, what the HTML uses, and the
/// bytes of both.
fn tally(files: &[Value]) -> (Vec<Value>, Vec<Value>, i64) {
    let mut shown = Vec::new();
    let mut parts = Vec::new();
    let mut bytes: i64 = 0;
    for file in files {
        // The bytes come from a `.meta.json` on disk. jq added them as numbers
        // of its own and could not overflow; a run of them that does is a bad
        // figure, not a reason to stop the lab, so it tops out.
        bytes = bytes.saturating_add(bytes_of(file));
        if text_of(file, "kind") == "part" {
            parts.push(file.clone());
        } else {
            shown.push(file.clone());
        }
    }
    (shown, parts, bytes)
}

/// The page of one ticket.
/// The page of one ticket: the files it shows, the ones its HTML uses, and the
/// ones the lab did not take. The ticket's own address is a name with a domain
/// and a path without one, which is what the menu and the row of the list use.
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
    let issue = issue_link(&text_of(row, "url"));
    let menu = issue
        .as_deref()
        .map_or(String::new(), |url| format!("      <a class=\"repo\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">TICKET ON GITHUB</a>\n", h(&Value::from(url))));
    let button = issue.as_deref().map_or(String::new(), |url| format!("        <p><a class=\"button button-primary\" href=\"{}\" target=\"_blank\" rel=\"noopener noreferrer\">OPEN THE TICKET ON GITHUB</a></p>\n", h(&Value::from(url))));
    let opened = match row.get("author") {
        Some(Value::String(name)) if !name.is_empty() => h(&Value::from(name.as_str())),
        _ => "-".to_string(),
    };
    let (shown, parts, bytes) = tally(files);
    let mut out = top(&format!("#{number} {title}"), &menu, home, design);
    out.push_str(&format!(
        "    <section class=\"lab-ticket\" aria-labelledby=\"ticket-title\">\n      <p class=\"lab-box-head\"><span>PROTOTYPE</span><b>#{number}</b></p>\n      <div class=\"lab-ticket-body\">\n        <h1 id=\"ticket-title\">{}</h1>\n        <dl class=\"lab-facts\">\n          <div><dt>OPENED BY</dt><dd>{opened}</dd></div>\n          <div><dt>CHANGED</dt><dd>{}</dd></div>\n        </dl>\n",
        h(&Value::from(title.as_str())),
        day(&text_of(row, "updatedAt")),
    ));
    out.push_str(&button);
    out.push_str(&format!(
        "      </div>\n    </section>\n    <section class=\"lab-box\" id=\"assets\" aria-labelledby=\"assets-title\">\n      <div class=\"lab-box-head\"><h2 id=\"assets-title\">ASSETS</h2><p>{}, {} OF {} {}</p></div>\n      <div class=\"lab-box-body\">\n",
        plural(files.len() as i64, "FILE", "FILES"),
        size(bytes).to_ascii_uppercase(),
        size(limit).to_ascii_uppercase(),
        meter(bytes, limit),
    ));
    if files.is_empty() {
        out.push_str("        <p class=\"lab-note\">No image, video or HTML file is attached to this ticket. Attach one to the issue on GitHub and it is here within a few minutes.</p>\n");
    }
    for file in &shown {
        out.push_str(&asset(
            &text_of(file, "name"),
            &text_of(file, "kind"),
            bytes_of(file),
        ));
    }
    if !parts.is_empty() {
        out.push_str(
            "        <div class=\"lab-list\"><h3>FILES THE HTML USES</h3>\n        <ul>\n",
        );
        for part in &parts {
            out.push_str(&part_line(part));
        }
        out.push_str("        </ul></div>\n");
    }
    if !skipped.is_empty() || outside > 0 {
        out.push_str("        <div class=\"lab-list\"><h3>LEFT OUT</h3>\n        <ul>\n");
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
    out.push_str(&bottom(limit));
    out
}

/// One row of the lab's list.
fn row_of(item: &Value, domain: &str) -> String {
    let number = number_of(item);
    let files = count_of(item);
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
            row.insert("files".into(), Value::from(count_of(item)));
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
    use std::fs;

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

    /// The links of `text` as jq reads them with the rule of
    /// console/attach/src/link.rs. The rule is read from that file when the
    /// test runs, so this compares two readings and not two copies of one. With
    /// no jq on the host there is nothing to compare, and the Rust side is
    /// pinned by every other case of links here.
    fn links_of_the_rule(text: &str) -> Vec<String> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/attach/src/link.rs");
        let source = fs::read_to_string(path).expect("console/attach/src/link.rs is there");
        let rule = source
            .lines()
            .find_map(|line| line.strip_prefix("pub const LINK_RULE: &str = r\""))
            .and_then(|tail| tail.strip_suffix("\";"))
            .expect("the rule of the links is in console/attach/src/link.rs");
        let program = format!("def links: [scan({})];", Value::from(rule));
        let Ok(answered) = std::process::Command::new("jq")
            .args(["-Rs", "--arg", "t", text])
            .arg(format!("{program} $t | links"))
            .output()
        else {
            return Vec::new();
        };
        assert!(
            answered.status.success(),
            "jq: {}",
            String::from_utf8_lossy(&answered.stderr)
        );
        serde_json::from_slice(&answered.stdout).expect("jq answers a list of links")
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
    fn a_value_of_github_cannot_start_a_tag() {
        assert_eq!(
            h(&Value::from("<a href=\"x\">&'</a>")),
            "&lt;a href=&quot;x&quot;&gt;&amp;&#39;&lt;/a&gt;"
        );
        assert_eq!(h(&Value::Null), "");
        assert_eq!(h(&Value::from(7)), "7");
        assert_eq!(h(&Value::from(true)), "true");
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
    fn a_ticket_is_linked_by_name_only_with_a_domain() {
        assert_eq!(door(7, ""), "/7/");
        assert_eq!(door(7, "example.org"), "https://lab-7.example.org/");
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
        let (shown, parts, bytes) = tally(&files());
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
        // and the pass goes on to the next ticket.
        let huge = lines(
            "{\"name\":\"a\",\"kind\":\"image\",\"bytes\":9223372036854775807}\n{\"name\":\"b\",\"kind\":\"image\",\"bytes\":9223372036854775807}\n",
        );
        let (shown, parts, bytes) = tally(&huge);
        assert_eq!((shown.len(), parts.len()), (2, 0));
        assert_eq!(bytes, i64::MAX);
        assert_eq!(size(bytes), "8796093022208 MB");
        // A page of them is a page: it says its bytes and holds both files.
        let page = ticket_page(
            &value_of(r#"{"number":7,"title":"Big","url":"u"}"#),
            &huge,
            &[],
            0,
            "/",
            "",
            LIMIT,
        );
        assert!(page.contains("2 FILES, 8796093022208 MB OF 10 MB"));
        assert!(page.contains("<figcaption><span class=\"asset-name\">a</span>"));
        assert!(page.contains("<figcaption><span class=\"asset-name\">b</span>"));
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
    fn a_ticket_with_nothing_attached_says_so() {
        let page = ticket_page(&ticket(), &[], &[], 0, "/", "", LIMIT);
        assert!(page.contains("No image, video or HTML file is attached"));
        assert!(page.contains("0 FILES, 0 B OF 10 MB"));
        assert!(!page.contains("LEFT OUT"));
        assert!(!page.contains("FILES THE HTML USES"));
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
    fn a_ticket_no_one_opened_has_a_dash_for_its_author() {
        let mut row = ticket();
        row["author"] = Value::Null;
        assert!(ticket_page(&row, &[], &[], 0, "/", "", LIMIT).contains("<dd>-</dd>"));
        row["author"] = Value::from("");
        assert!(ticket_page(&row, &[], &[], 0, "/", "", LIMIT).contains("<dd>-</dd>"));
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
            "example.org",
            "https://lab.example.org/",
            "",
            LIMIT,
        );
        assert!(named.contains("href=\"https://lab-7.example.org/\""));
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
            "example.org",
            LIMIT,
        );
        let read: Value = serde_json::from_str(&named).unwrap();
        assert_eq!(read["url"], "https://lab.example");
        assert_eq!(read["limitBytes"], LIMIT);
        assert_eq!(read["generatedAt"], "2026-10-28T09:00:00Z");
        assert_eq!(read["tickets"][0]["address"], "https://lab-7.example.org/");
        // When the page was built, as the pass read it off the page.
        let built = lines(
            r#"{"number":7,"title":"Map","url":"u","files":[],"bytes":0,"builtAt":"2026-10-28T08:00:00Z"}"#,
        );
        let read: Value =
            serde_json::from_str(&live_file(&built, "2026-10-28T09:00:00Z", "", "", LIMIT))
                .unwrap();
        assert_eq!(read["tickets"][0]["builtAt"], "2026-10-28T08:00:00Z");
    }

    fn alice(who: &str) -> bool {
        who == "alice"
    }

    /// An issue as the texts read answers it: its body and its comments, each
    /// with its author and, when edited, its last editor.
    fn issue(body: &str, author: &str, editor: Option<&str>, comments: Value) -> Value {
        serde_json::json!({
            "body": body,
            "author": {"login": author},
            "editor": editor.map(|e| serde_json::json!({"login": e})),
            "comments": {"nodes": comments},
        })
    }

    fn comment(body: &str, author: &str, editor: Option<&str>) -> Value {
        serde_json::json!({
            "body": body,
            "author": {"login": author},
            "editor": editor.map(|e| serde_json::json!({"login": e})),
        })
    }

    const SHOT: &str =
        "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555";

    #[test]
    fn only_allowed_accounts_give_links_to_fetch() {
        let body = format!(
            "shot [x]({SHOT}) and [again]({SHOT}) and {}",
            "https://github.com/acme/widget/files/21/proto.html"
        );
        let comments = serde_json::json!([
            comment(
                "https://github.com/user-attachments/files/31/vid.mp4",
                "alice",
                None
            ),
            comment(
                "[c](https://github.com/user-attachments/files/32/collab.html)",
                "carol",
                None
            ),
            comment(
                "[b](https://github.com/user-attachments/files/33/bot.html)",
                "tehas-bot[bot]",
                None
            ),
        ]);
        let got = refs(&issue(&body, "alice", None, comments), &alice, false);
        assert_eq!(
            got.fetch,
            [
                SHOT,
                "https://github.com/acme/widget/files/21/proto.html",
                "https://github.com/user-attachments/files/31/vid.mp4",
            ]
        );
        // A collaborator off the list and the bot are both outside.
        assert_eq!(got.outside, 2);
    }

    #[test]
    fn a_text_someone_else_edited_last_gives_nothing_to_fetch() {
        let link = "https://github.com/user-attachments/files/40/theirs.html";
        let comments = serde_json::json!([
            comment(&format!("one {link}"), "alice", Some("tehas-bot[bot]")),
            comment(
                "https://github.com/user-attachments/files/9/mine.html",
                "alice",
                Some("alice")
            ),
        ]);
        let got = refs(
            &issue(SHOT, "alice", Some("mallory"), comments),
            &alice,
            false,
        );
        assert_eq!(
            got.fetch,
            ["https://github.com/user-attachments/files/9/mine.html"]
        );
        assert_eq!(got.outside, 2);
        // An editor with no login is nobody on the list.
        let ghost = serde_json::json!({"body": SHOT, "author": {"login": "alice"}, "editor": {}});
        assert!(refs(&ghost, &alice, false).fetch.is_empty());
    }

    #[test]
    fn a_link_on_a_quoted_line_is_someone_elses() {
        let theirs = "https://github.com/user-attachments/files/41/theirs.html";
        let body = format!("> {theirs}\n  >> [x]({theirs})\nmine: {SHOT}");
        let got = refs(
            &issue(&body, "alice", None, serde_json::json!([])),
            &alice,
            false,
        );
        assert_eq!(got.fetch, [SHOT]);
        assert_eq!(got.outside, 1);
    }

    #[test]
    fn a_text_with_no_author_or_no_comments_is_read_as_far_as_it_goes() {
        let got = refs(
            &serde_json::json!({"body": SHOT, "author": null}),
            &alice,
            false,
        );
        assert!(got.fetch.is_empty());
        assert_eq!(got.outside, 1);
        let got = refs(
            &serde_json::json!({"body": SHOT, "author": {"login": "alice"}}),
            &alice,
            false,
        );
        assert_eq!(got.fetch, [SHOT]);
    }

    #[test]
    fn the_key_of_the_refs_follows_the_title_the_links_and_the_rest() {
        let a = Refs {
            fetch: vec![SHOT.to_string()],
            zips: Vec::new(),
            outside: 0,
        };
        let b = Refs {
            fetch: vec![SHOT.to_string()],
            zips: Vec::new(),
            outside: 1,
        };
        assert_eq!(a.key("Map"), a.key("Map"));
        assert_ne!(a.key("Map"), a.key("Maps"));
        assert_ne!(a.key("Map"), b.key("Map"));
        assert!(!a.key("Map").contains(' '));
        let zipped = |sha256: &str| Refs {
            fetch: vec![SHOT.to_string()],
            zips: vec![Ok(Zip {
                link: ZIP.to_string(),
                sha256: sha256.to_string(),
            })],
            outside: 0,
        };
        assert_ne!(zipped(&SUM).key("Map"), a.key("Map"));
        assert_ne!(zipped(&SUM).key("Map"), zipped(&"b".repeat(64)).key("Map"));
    }

    const ZIP: &str = "https://files.example.org/p/proto.zip";
    static SUM: std::sync::LazyLock<String> = std::sync::LazyLock::new(|| "a".repeat(64));

    #[test]
    fn a_zip_line_names_its_link_and_sha256() {
        let zip = Zip {
            link: ZIP.to_string(),
            sha256: SUM.clone(),
        };
        assert_eq!(
            zip_mark(&format!("Zip: {ZIP} sha256:{}", *SUM)),
            Some(Ok(zip.clone()))
        );
        assert_eq!(
            zip_mark(&format!("  Zip:\t{ZIP}   sha256:{}  \r", *SUM)),
            Some(Ok(zip))
        );
        for line in [
            "",
            "zip: x sha256:y",
            "A Zip: mid-line",
            "- Zip: a list item",
        ] {
            assert_eq!(zip_mark(line), None, "{line:?}");
        }
        for (line, word) in [
            ("Zip:".to_string(), ""),
            (format!("Zip: {ZIP}"), ZIP),
            (format!("Zip: {ZIP} {}", *SUM), ZIP),
            (format!("Zip: {ZIP} sha256:{}", SUM.to_uppercase()), ZIP),
            (format!("Zip: {ZIP} sha256:{}", &SUM[1..]), ZIP),
            (format!("Zip: {ZIP} x sha256:{}", *SUM), ZIP),
        ] {
            assert_eq!(zip_mark(&line), Some(Err(word.to_string())), "{line:?}");
        }
    }

    #[test]
    fn a_zip_line_is_read_only_with_the_feature_and_by_the_rule_of_links() {
        let line = format!("Zip: {ZIP} sha256:{}", *SUM);
        let github = format!(
            "Zip: https://github.com/user-attachments/files/5/p.zip sha256:{}",
            *SUM
        );
        let body = format!("{line}\n{line}\n> {line}\n{github}\nZip: nonsense");
        let comments = serde_json::json!([comment(
            &format!("Zip: https://files.example.org/theirs.zip sha256:{}", *SUM),
            "mallory",
            None
        )]);
        let ticket = issue(&body, "alice", None, comments);
        let off = refs(&ticket, &alice, false);
        assert!(off.zips.is_empty());
        // Off, a Zip: line is text as before: its GitHub link is an attachment.
        assert_eq!(
            off.fetch,
            ["https://github.com/user-attachments/files/5/p.zip"]
        );
        assert_eq!(off.outside, 0);
        let on = refs(&ticket, &alice, true);
        assert!(on.fetch.is_empty());
        assert_eq!(
            on.zips,
            [
                Ok(Zip {
                    link: ZIP.to_string(),
                    sha256: SUM.clone()
                }),
                Ok(Zip {
                    link: "https://github.com/user-attachments/files/5/p.zip".to_string(),
                    sha256: SUM.clone()
                }),
                Err("nonsense".to_string()),
            ]
        );
        // The quoted line and mallory's line.
        assert_eq!(on.outside, 2);
    }

    #[test]
    fn only_githubs_own_storage_is_a_link_and_nothing_else() {
        let text = format!(
            "{} {} {} {} {} {}",
            "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555",
            "https://github.com/user-attachments/files/21/proto.html",
            "https://github.com/acme/widget/files/22/style.css",
            "https://acme.github.io/widget/files/23/elsewhere.css",
            "https://user-images.githubusercontent.com/12345/98765-abcd-ef01-2345-6789abcdef01",
            "https://example.com/not-ours.png",
        );
        assert_eq!(
            links(&text),
            [
                "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555",
                "https://github.com/user-attachments/files/21/proto.html",
                "https://github.com/acme/widget/files/22/style.css",
                "https://user-images.githubusercontent.com/12345/98765-abcd-ef01-2345-6789abcdef01",
            ]
        );
        for nothing in [
            "https://github.com/",
            "https://github.com/a/b/other/1/x.html",
            "https://github.com/a/b/files/x/y.html",
            "https://github.com/a/b",
            "https://githubuser-attachments/assets/1",
            "https://example.com/assets/1",
            "https://github.com/a/b/files/1a/x.html",
        ] {
            let none: Vec<String> = Vec::new();
            assert_eq!(links(nothing), none, "{nothing}");
        }
    }

    #[test]
    fn a_name_of_two_parts_may_hold_dots_hyphens_and_underscores() {
        assert_eq!(
            links("https://github.com/ac-me.my_org/w-id.get/files/7/x.js"),
            ["https://github.com/ac-me.my_org/w-id.get/files/7/x.js"]
        );
    }

    #[test]
    fn a_link_ends_before_a_part_that_would_leave_the_folder() {
        let cases = [
            "https://github.com/o/r/files/1/../../raw/main/a.json",
            "https://github.com/o/r/files/1/./a.json",
            "https://github.com/o/r/files/1/%2e%2e/raw/a.json",
            "https://github.com/o/r/files/1/.%2E/raw/a.json",
            "https://github.com/o/r/files/1/..?x=1",
            "https://github.com/o/../files/1/a.json",
            "https://github.com/user-attachments/files/../../o/r/raw/a.json",
            "https://user-images.githubusercontent.com/../x.png",
            "https://github.com/user-attachments/files/1/../../o/r/raw/a.json",
            "https://github.com/o/r/files/1/a/../../../raw/a.json",
            "https://github.com/o/r/files/1/a%2f..%2fb.json",
            "https://github.com/o/r/files/1/a%5C..%5cb.json",
            "https://github.com/o/r/files/1/.hidden.json",
            "https://github.com/o/r/files/1/a..b/...c.json",
            "https://github.com/o.r/r.s/files/1/a%20b.json?x=../y",
        ];
        let text = cases.join(" ");
        let found = links(&text);
        assert_eq!(
            found,
            [
                "https://github.com/user-attachments/files/1",
                "https://github.com/o/r/files/1/a",
                "https://github.com/o/r/files/1/.hidden.json",
                "https://github.com/o/r/files/1/a..b/...c.json",
                "https://github.com/o.r/r.s/files/1/a%20b.json?x=../y",
            ]
        );
        for link in &cases[..12] {
            assert!(!attach::link::is_link(link), "{link}");
        }
        // jq's scan lists a link each time it is found.
        let mut said = links_of_the_rule(&text);
        said.dedup();
        assert!(said.is_empty() || said == found, "{said:?}");
    }

    #[test]
    fn a_link_ends_where_the_address_does() {
        // The bytes of the link end where a URL's do: ',' and ' ' and ')'
        // stand outside it, a fragment is no part of the address.
        assert_eq!(
            links("x https://github.com/user-attachments/files/1/a.png?foo=bar&x=1,baz, https://github.com/a/b/files/2/c.js#frag y"),
            [
                "https://github.com/user-attachments/files/1/a.png?foo=bar&x=1",
                "https://github.com/a/b/files/2/c.js",
            ]
        );
        // The same cut with jq's own reading of the rule, so the two cannot
        // drift: the line is taken from the file every time this test runs.
        let cases = "x https://github.com/user-attachments/files/1/a.png?foo=bar&x=1,baz, https://github.com/a/b/files/2/c.js#frag y";
        let said = links_of_the_rule(cases);
        assert_eq!(said, links(cases));
        // Markdown's bracket is no URL character, and a link may stand alone.
        assert_eq!(
            links("![shot](https://github.com/user-attachments/files/2/a.png)"),
            ["https://github.com/user-attachments/files/2/a.png"]
        );
    }
}
