//! The Protolab's pages, for `tehasbot protolab`. libexec/protolab keeps the
//! settings, the GitHub calls and the downloads; this turns what it read into
//! the files the edge router serves.
//!
//!   console protolab page <ticket> <files> <skipped> <outside> <home> <limit>
//!       The page of one ticket, printed in full. <ticket> is its JSON -
//!       {number, title, url, author, updatedAt} - as the shell read it. The
//!       two lists are files and skipped, each one line of JSON,
//!       {"name","kind","bytes"} and {"name","reason"}; a file's name holds a
//!       slash when it came out of a zip. Either list is a file to read or the
//!       lines themselves. <outside> counts the attachment links of people
//!       outside the repository, <home> the address the list is at, <limit> the
//!       bytes a ticket may hold.
//!   console protolab front <metas> <domain> <home> <limit>
//!       The list at the front of the lab: one line of ticket JSON each under
//!       <metas>, a file of them or the lines themselves.
//!   console protolab live <metas> <now> <url> <domain> <limit>
//!       live/protolab.json, the list the dashboard links from: the same
//!       lines, <now> the time this pass read them, <url> TEHAS_PROTOLAB_URL.
//!   console protolab refs - -
//!       The attachment links of one ticket: {"refs":[...],"outside":n} on
//!       standard output. The ticket and its comments are given on standard
//!       input, both of them, the ticket first and then the list of its
//!       replies; each is a JSON value of its own, the replies read as the
//!       whole of the text after the ticket. Only text by an owner, member or
//!       collaborator gives links to fetch; what anyone outside the repository
//!       wrote is counted, not fetched.
//!
//! The files beside a ticket are fetched and written by the shell: this only
//! names them under files/ and says what a page holds. Every value that came
//! from GitHub is escaped, so no title, name or reason can start a tag of its
//! own. Nothing here writes a file or calls anything.
//!
//! Exit: 0 what was asked for was printed; 1 standard output could not be
//! written; 64 a usage error, a ticket that is not one, or a figure that is
//! not a whole number.
//!
//! The shape of every page here is what test/protolab_test.sh pins, line for
//! line, so that the pages of a host of this tehas are the pages of the host
//! before it.

use serde_json::{Map, Value};
use std::cell::{Cell, RefCell};
use std::fs;
use std::io::{self, Read, Write};

const USAGE: &str = "usage: console protolab page <ticket> <files> <skipped> <outside> <home> <limit> | front <metas> <domain> <home> <limit> | live <metas> <now> <url> <domain> <limit> | refs - -";

pub fn main(given: &[String]) -> u8 {
    let arg = |i: usize| given.get(i).cloned().unwrap_or_default();
    let said = match given.first().map(String::as_str) {
        Some("page") if given.len() == 7 => {
            page(&arg(1), &arg(2), &arg(3), &arg(4), &arg(5), &arg(6))
        }
        Some("front") if given.len() == 5 => {
            metas(&arg(1)).map(|tickets| front(&tickets, &arg(2), &arg(3), whole(&arg(4))))
        }
        Some("live") if given.len() == 6 => {
            metas(&arg(1)).map(|tickets| live(&tickets, &arg(2), &arg(3), &arg(4), whole(&arg(5))))
        }
        Some("refs") if given.len() == 3 && given[1] == "-" && given[2] == "-" => refs(),
        _ => {
            eprintln!("{USAGE}");
            return 64;
        }
    };
    match said {
        Ok(text) => {
            let mut out = io::stdout();
            if out
                .write_all(text.as_bytes())
                .and_then(|()| out.flush())
                .is_err()
            {
                eprintln!("console protolab: standard output could not be written");
                return 1;
            }
            0
        }
        Err(why) => {
            eprintln!("console protolab: {why}");
            64
        }
    }
}

/// A whole number, and nothing else: a figure of the page, never a word.
fn whole(text: &str) -> i64 {
    text.trim().parse().unwrap_or(0)
}

/// A file to read, or the text itself when no file stands at that path: a
/// page's lists are short enough for an argument and long enough for a file.
fn read_back(text_or_path: &str) -> String {
    if text_or_path == "-" {
        return whole_of_stdin();
    }
    match fs::read(text_or_path) {
        Ok(bytes) => String::from_utf8_lossy(&bytes).into_owned(),
        // What is there cannot be read as text, so it is no file to read: the
        // bytes of the argument are what was asked for.
        Err(_) => text_or_path.to_string(),
    }
}

/// Every text the shell gave on standard input, read once and kept: a verb that
/// takes two of them reads the one descriptor once. Nothing was given when the
/// descriptor has nothing to give.
fn whole_of_stdin() -> String {
    if GIVEN.with(|asked| !asked.replace(true)) {
        let mut text = String::new();
        // A closed or broken descriptor is no text: what was asked for is then
        // nothing, and the verb says so in its own words.
        let _ = io::stdin().read_to_string(&mut text);
        STDIN.with(|held| held.replace(text));
    }
    STDIN.with(|held| held.borrow().clone())
}

thread_local! {
    static STDIN: RefCell<String> = const { RefCell::new(String::new()) };
    static GIVEN: Cell<bool> = const { Cell::new(false) };
}

/// What a test gives in place of the shell's standard input.
#[cfg(test)]
fn give(text: &str) {
    STDIN.with(|held| held.replace(text.to_string()));
    GIVEN.with(|held| held.set(true));
}

/// The value of a whole text: a list, an object, nothing when it is no JSON.
fn value_of(text: &str) -> Value {
    serde_json::from_str(text.trim()).unwrap_or(Value::Null)
}

/// The first JSON value of a text, and nothing of what follows it: the two
/// texts the shell gives are read in one go, and each is a value of its own. A
/// bracket inside a string is no bracket, which is what the walk of the text
/// has to know before it counts one.
fn first_value(text: &str) -> Option<Value> {
    let (start, end) = value_span(text)?;
    serde_json::from_str(&text[start..end]).ok()
}

/// The last JSON value of a text: where it begins and where the text ends, so
/// everything after the first value belongs to it, brackets and all.
fn last_value(text: &str) -> Option<Value> {
    let after = value_span(text)?;
    let rest = text[after.1..].trim();
    if rest.is_empty() {
        return None;
    }
    serde_json::from_str(rest).ok()
}

/// The front of the first JSON value of a text and the byte one past it. The
/// walk counts brackets that are not inside a string, so it finds where the
/// value ends without reading it twice.
fn value_span(text: &str) -> Option<(usize, usize)> {
    let bytes = text.as_bytes();
    let mut start = usize::MAX;
    let mut depth = 0i32;
    let mut quoted = false;
    let mut escaped = false;
    for (at, byte) in bytes.iter().enumerate() {
        if start == usize::MAX {
            if *byte == b'{' || *byte == b'[' {
                start = at;
                depth = 1;
            }
            continue;
        }
        if quoted {
            if escaped {
                escaped = false;
            } else if *byte == b'\\' {
                escaped = true;
            } else if *byte == b'"' {
                quoted = false;
            }
            continue;
        }
        match byte {
            b'"' => quoted = true,
            b'{' | b'[' => depth += 1,
            b'}' | b']' => {
                depth -= 1;
                if depth == 0 {
                    return Some((start, at + 1));
                }
            }
            _ => {}
        }
    }
    None
}

/// The lines of a file of JSON, one value each; a line that is no JSON says
/// nothing, so one broken page does not cost the list its other rows. A whole
/// text that is one list of its own is read as that list.
fn lines(text: &str) -> Vec<Value> {
    if let Value::Array(rows) = value_of(text) {
        return rows;
    }
    text.lines()
        .filter(|line| !line.trim().is_empty())
        .map(value_of)
        .filter(|row| !row.is_null())
        .collect()
}

/// The page of one ticket, printed whole.
fn page(
    ticket: &str,
    files: &str,
    skipped: &str,
    outside: &str,
    home: &str,
    limit: &str,
) -> Result<String, String> {
    let row = match value_of(&read_back(ticket)) {
        Value::Object(row) => Value::Object(row),
        _ => return Err("the ticket is not a ticket".to_string()),
    };
    let files = lines(&read_back(files));
    let skipped = lines(&read_back(skipped));
    Ok(ticket_page(
        &row,
        &files,
        &skipped,
        whole(outside),
        home,
        whole(limit),
    ))
}

/// The lab's front page.
fn front(metas: &[Value], domain: &str, home: &str, limit: i64) -> String {
    front_page(metas, domain, home, limit)
}

/// live/protolab.json, the list the dashboard links from.
fn live(metas: &[Value], now: &str, url: &str, domain: &str, limit: i64) -> String {
    live_file(metas, now, url, domain, limit)
}

/// Every ticket of the lab, newest first, whatever order the file held them
/// in: the number is what the pages are named by.
fn metas(path: &str) -> Result<Vec<Value>, String> {
    let mut tickets = lines(&read_back(path));
    tickets.sort_by_key(|row| std::cmp::Reverse(number_of(row)));
    Ok(tickets)
}

/// The attachment links of one ticket, and how many came from outside.
///
/// The shell gives both texts on standard input, the ticket first and then the
/// list of its replies: one read of one descriptor, so a caller needs no
/// temporary file and no argument long enough to break. The replies are read as
/// the whole of the text after the ticket, so a body of its own may hold a
/// newline or any bracket.
fn refs() -> Result<String, String> {
    let both = read_back("-");
    let row = match first_value(&both) {
        Some(Value::Object(row)) => Value::Object(row),
        _ => return Err("the ticket is not a ticket".to_string()),
    };
    let list = match last_value(&both) {
        Some(Value::Array(rows)) => rows,
        Some(Value::Object(reply)) => vec![Value::Object(reply)],
        _ => return Err("the comments are not a list".to_string()),
    };
    // The text of the ticket is written by its author, so the association of
    // the issue answers for it, under either of the two names it is read by.
    let mine_word = association(&row, &["association", "author_association"]);
    let mut mine = Vec::new();
    let mut theirs = Vec::new();
    let mut texts: Vec<(String, String)> = vec![(mine_word, text_of(&row, "body"))];
    for reply in &list {
        texts.push((
            association(reply, &["author_association", "association"]),
            text_of(reply, "body"),
        ));
    }
    for (who, text) in texts {
        let ours = matches!(who.as_str(), "OWNER" | "MEMBER" | "COLLABORATOR");
        for link in links(&text) {
            let keep = if ours { &mut mine } else { &mut theirs };
            if !keep.contains(&link) {
                keep.push(link);
            }
        }
    }
    let mut said = String::from("{\"refs\":[");
    // The two keys in the order the file has always held them, and its links
    // in the order the ticket wrote them.
    for (i, link) in mine.iter().enumerate() {
        if i > 0 {
            said.push(',');
        }
        said.push_str(&Value::from(link.as_str()).to_string());
    }
    said.push_str("],\"outside\":");
    said.push_str(&theirs.len().to_string());
    said.push_str("}\n");
    Ok(said)
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
    let b = updated.as_bytes();
    let digits = |r: std::ops::Range<usize>| {
        b.get(r.clone())
            .is_some_and(|s| s.iter().all(u8::is_ascii_digit))
    };
    if b.len() != 20
        || !digits(0..4)
        || b[4] != b'-'
        || !digits(5..7)
        || b[7] != b'-'
        || !digits(8..10)
        || b[10] != b'T'
        || !digits(11..13)
        || b[13] != b':'
        || !digits(14..16)
        || b[16] != b':'
        || !digits(17..19)
        || b[19] != b'Z'
    {
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
/// path without one. tehas_edge_name makes the same name for the shell.
fn door(number: i64, domain: &str) -> String {
    if domain.is_empty() {
        format!("/{number}/")
    } else {
        format!("https://lab-{number}.{domain}/")
    }
}

/// The head of a page: the style sheets of host/fun, the wordmark, the menu.
fn top(title: &str, menu: &str, home: &str) -> String {
    let home = h(&Value::from(home));
    let mut out = String::from("<!doctype html>\n<html lang=\"en\">\n<head>\n");
    out.push_str(&format!("  <meta charset=\"utf-8\">\n  <meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\n  <meta name=\"robots\" content=\"noindex, nofollow\">\n  <title>{} - Protolab</title>\n", h(&Value::from(title))));
    out.push_str("  <link rel=\"icon\" type=\"image/svg+xml\" href=\"/fun/logo.svg\">\n  <link rel=\"stylesheet\" href=\"/fun/theme.css\">\n  <link rel=\"stylesheet\" href=\"/fun/game-view.css\">\n  <link rel=\"stylesheet\" href=\"/fun/protolab.css\">\n");
    out.push_str("</head>\n<body id=\"top\">\n  <a class=\"skip\" href=\"#page\">Skip to the page</a>\n  <header class=\"topbar topbar-short\">\n");
    out.push_str(&format!("    <a class=\"brand\" href=\"{home}\" aria-label=\"Protolab, every prototype\">\n      <img src=\"/fun/logo.svg\" width=\"50\" height=\"50\" alt=\"\">\n      <span>PROTOLAB</span>\n    </a>\n    <nav class=\"menu\" aria-label=\"Site\">\n      <a href=\"{home}\">ALL PROTOTYPES</a>\n"));
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

/// The first of these keys that holds the words of one text: GitHub answers the
/// same fact under the name of the read that asked for it.
fn association(row: &Value, keys: &[&str]) -> String {
    keys.iter()
        .map(|key| text_of(row, key))
        .find(|seen| !seen.is_empty())
        .unwrap_or_default()
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
    let mut out = top(&format!("#{number} {title}"), &menu, home);
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
                "          <li><span>{} from outside the repository</span><span class=\"asset-fact\">Only files attached by an owner, member or collaborator are taken.</span></li>\n",
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
fn front_page(tickets: &[Value], domain: &str, home: &str, limit: i64) -> String {
    let mut out = top("Prototypes", "", home);
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

// --- The links of a ticket --------------------------------------------------

/// Every link of GitHub's own attachment storage in a text, in the order it was
/// written: the same shapes tehas_attachment_links of lib/attachments.sh gives,
/// which are the only addresses the lab ever fetches.
///
/// The scan walks the text a byte at a time and asks each position whether a
/// scan of that regular expression may begin there, which is what the
/// leftmost-match rule of Oniguruma does.
fn links(text: &str) -> Vec<String> {
    let bytes = text.as_bytes();
    let mut found = Vec::new();
    let mut at = 0;
    while at < bytes.len() {
        match address(bytes, at) {
            Some(end) if end > at => {
                let link = &text[at..end];
                if !found.iter().any(|seen| seen == link) {
                    found.push(link.to_string());
                }
                at = end;
            }
            _ => at += 1,
        }
    }
    found
}

const ATTACHMENTS: &str = "https://github.com/user-attachments/";
const IMAGES: &str = "https://user-images.githubusercontent.com/";
const GITHUB: &str = "https://github.com/";

/// The byte one past the link that begins at `at`, or None when no branch of
/// the regular expression of lib/attachments.sh matches there. Each branch is
/// the host, the folders that say the file is GitHub's own, and then a run of
/// URL characters that holds the file.
fn address(bytes: &[u8], at: usize) -> Option<usize> {
    if let Some(tail) = rest(bytes, at).strip_prefix(ATTACHMENTS) {
        // An attachment of the issue: assets/ or files/, and the file.
        if tail.starts_with("assets/") || tail.starts_with("files/") {
            return Some(run(bytes, at + ATTACHMENTS.len() + "assets/".len()));
        }
        return None;
    }
    if rest(bytes, at).starts_with(IMAGES) {
        // A pasted image: the host says it all, the name of the file follows.
        return Some(run(bytes, at + IMAGES.len()));
    }
    if rest(bytes, at).starts_with(GITHUB) {
        // A file of a repository: the owner, the repository, the folder and
        // the number that holds it.
        let (_, tail) = name(&rest(bytes, at)[GITHUB.len()..])?;
        let (_, tail) = name(tail)?;
        let tail = tail
            .strip_prefix("assets/")
            .or_else(|| tail.strip_prefix("files/"))?;
        let held = tail.find('/')?;
        if held == 0 || !tail[..held].bytes().all(|b| b.is_ascii_digit()) {
            return None;
        }
        // The file begins past the number, and its name runs to the end of the
        // URL characters that follow it.
        let past = rest(bytes, at).len() - tail.len() + held + 1;
        return Some(run(bytes, at + past));
    }
    None
}

/// The rest of the text from `at`, empty when nothing is there.
fn rest(bytes: &[u8], at: usize) -> &str {
    std::str::from_utf8(bytes.get(at..).unwrap_or(b"")).unwrap_or_default()
}

/// A repository or an owner's name at the front of `tail`: letters, digits and
/// . _ -, at least one, then the slash that ends it. The name and what follows
/// the slash.
fn name(tail: &str) -> Option<(&str, &str)> {
    let cut = tail.find('/')?;
    let (seen, after) = (&tail[..cut], &tail[cut + 1..]);
    (!seen.is_empty()
        && seen
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'_' | b'.' | b'-')))
    .then_some((seen, after))
}

/// The byte one past the run of URL characters at `at`. A fragment is no part
/// of where GitHub holds a file, so the run stops at one.
fn run(bytes: &[u8], at: usize) -> usize {
    let mut end = at;
    while bytes.get(end).is_some_and(|b| {
        b.is_ascii_alphanumeric()
            || matches!(
                b,
                b'.' | b'_' | b'~' | b'%' | b'/' | b'?' | b'&' | b'=' | b'+' | b'-'
            )
    }) {
        end += 1;
    }
    end
}

#[cfg(test)]
mod tests {
    use super::*;

    const LIMIT: i64 = 10 * 1024 * 1024;

    /// The links of `text` as jq reads them with the program of
    /// lib/attachments.sh. The definition is read from that file when the test
    /// runs, so this compares two readings and not two copies of one. With no
    /// jq on the host there is nothing to compare, and the Rust side is pinned
    /// by every other case of links here.
    fn links_of_the_shell(text: &str) -> Vec<String> {
        let path = concat!(env!("CARGO_MANIFEST_DIR"), "/../lib/attachments.sh");
        let source = fs::read_to_string(path).expect("lib/attachments.sh is there");
        let held = source
            .lines()
            .find(|line| line.contains("def links:"))
            .expect("the definition of links is in lib/attachments.sh");
        // The printf of the function holds the program between two quotes,
        // after the one that quotes the format of the printf.
        let program = held.split('\'').nth(3).expect("the program of the printf");
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
        assert!(page.contains("1 link from outside the repository</span>"));
        assert!(page.contains("<figcaption><span class=\"asset-name\">image-1.png</span>"));
        assert!(page.contains("aria-label=\"Open proto.html by itself\">OPEN FULL SIZE</a>"));
        assert!(page.contains("09 OCT 2026"));
        assert!(page.ends_with("</body>\n</html>"));
    }

    #[test]
    fn a_ticket_with_nothing_attached_says_so() {
        let page = ticket_page(&ticket(), &[], &[], 0, "/", LIMIT);
        assert!(page.contains("No image, video or HTML file is attached"));
        assert!(page.contains("0 FILES, 0 B OF 10 MB"));
        assert!(!page.contains("LEFT OUT"));
        assert!(!page.contains("FILES THE HTML USES"));
    }

    #[test]
    fn a_ticket_with_no_github_address_holds_no_link_to_github() {
        let mut row = ticket();
        row["url"] = Value::from("https://elsewhere.example/7");
        let page = ticket_page(&row, &[], &[], 0, "/", LIMIT);
        assert!(!page.contains("TICKET ON GITHUB"));
        assert!(!page.contains("OPEN THE TICKET"));
    }

    #[test]
    fn a_ticket_no_one_opened_has_a_dash_for_its_author() {
        let mut row = ticket();
        row["author"] = Value::Null;
        assert!(ticket_page(&row, &[], &[], 0, "/", LIMIT).contains("<dd>-</dd>"));
        row["author"] = Value::from("");
        assert!(ticket_page(&row, &[], &[], 0, "/", LIMIT).contains("<dd>-</dd>"));
    }

    #[test]
    fn the_list_names_every_ticket_and_says_when_there_is_none() {
        let one = value_of(
            r#"{"number":7,"title":"Map","url":"u","files":[{"name":"a","kind":"image","bytes":3}],"bytes":3}"#,
        );
        let page = front_page(&[one], "", "/", LIMIT);
        assert!(page.contains("<a class=\"row\" href=\"/7/\"><b>#7</b>"));
        assert!(page.contains("1 PROTOTYPE<"));
        assert!(page.contains("1 FILE, 3 B"));
        let none = front_page(&[], "", "/", LIMIT);
        assert!(none.contains("Nothing in the lab."));
        assert!(none.contains("0 PROTOTYPES<"));
        let named = front_page(
            &[value_of(
                r#"{"number":7,"title":"M","url":"u","files":[],"bytes":0}"#,
            )],
            "example.org",
            "https://lab.example.org/",
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
    }

    #[test]
    fn the_metas_are_read_newest_first_whatever_order_they_came_in() {
        let got: Vec<i64> = metas(
            "{\"number\":3,\"title\":\"c\"}\n{\"number\":11,\"title\":\"a\"}\n{\"number\":7,\"title\":\"b\"}\n",
        )
        .unwrap()
        .iter()
        .map(number_of)
        .collect();
        assert_eq!(got, [11, 7, 3]);
    }

    #[test]
    fn one_broken_line_does_not_cost_the_list_its_other_rows() {
        assert_eq!(lines("not json\n{\"a\":1}\n\n").len(), 1);
        assert_eq!(lines("[{\"a\":1},{\"b\":2}]").len(), 2);
        assert!(lines("").is_empty());
    }

    #[test]
    fn only_people_of_the_repository_give_links_to_fetch() {
        let row = value_of(&format!(
            "{{\"number\":7,\"author_association\":\"OWNER\",\"body\":\"shot [{}]({}) and [again]({}) and {}\"}}",
            "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555",
            "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555",
            "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555",
            "https://github.com/acme/widget/files/21/proto.html",
        ));
        let replies = value_of(
            r#"[{"author_association":"COLLABORATOR","body":"https://github.com/user-attachments/files/31/vid.mp4"},
                {"author_association":"NONE","body":"[stranger](https://github.com/user-attachments/files/32/stranger.html)"}]"#,
        );
        let mut texts: Vec<(String, String)> =
            vec![(text_of(&row, "author_association"), text_of(&row, "body"))];
        for reply in replies.as_array().unwrap() {
            texts.push((text_of(reply, "author_association"), text_of(reply, "body")));
        }
        let (mut mine, mut theirs) = (Vec::new(), Vec::new());
        for (who, text) in texts {
            let ours = matches!(who.as_str(), "OWNER" | "MEMBER" | "COLLABORATOR");
            for link in links(&text) {
                let keep = if ours { &mut mine } else { &mut theirs };
                if !keep.contains(&link) {
                    keep.push(link);
                }
            }
        }
        assert_eq!(
            mine,
            [
                "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555",
                "https://github.com/acme/widget/files/21/proto.html",
                "https://github.com/user-attachments/files/31/vid.mp4",
            ]
        );
        assert_eq!(
            theirs,
            ["https://github.com/user-attachments/files/32/stranger.html"]
        );
    }

    #[test]
    fn the_refs_of_a_ticket_are_its_links_and_the_count_of_the_rest() {
        let row = r#"{"number":7,"author_association":"NONE","body":"[theirs](https://github.com/user-attachments/files/40/theirs.html)"}"#;
        give(&format!("{row}\n[]\n"));
        let said = refs().unwrap();
        assert_eq!(said, "{\"refs\":[],\"outside\":1}\n".to_string());
        give("nope\n[]\n");
        assert!(refs().is_err());
        give(&format!("{row}\nnope\n"));
        assert!(refs().is_err());
        // A reply by a person of the repository gives a link to fetch, and the
        // body of one may hold a newline: the list is read as a whole.
        give(&format!(
            "{row}\n[{{\"author_association\":\"OWNER\",\"body\":\"one\\nhttps://github.com/user-attachments/files/9/one.html\"}}]\n"
        ));
        assert_eq!(
            refs().unwrap(),
            "{\"refs\":[\"https://github.com/user-attachments/files/9/one.html\"],\"outside\":1}\n"
        );
        // Nothing at all is no ticket, and a list with no ticket before it is
        // nothing to say.
        give("");
        assert!(refs().is_err());
        // A ticket alone is no answer: its replies are asked for as well, and
        // a list of them is what the command reads after the ticket.
        give(&format!("{row}\n"));
        assert!(refs().is_err());
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
        // The same cut with jq's own program, read from lib/attachments.sh
        // where it lives, so the two cannot drift: the line is taken from the
        // file every time this test runs.
        let cases = "x https://github.com/user-attachments/files/1/a.png?foo=bar&x=1,baz, https://github.com/a/b/files/2/c.js#frag y";
        let said = links_of_the_shell(cases);
        assert_eq!(said, links(cases));
        // Markdown's bracket is no URL character, and a link may stand alone.
        assert_eq!(
            links("![shot](https://github.com/user-attachments/files/2/a.png)"),
            ["https://github.com/user-attachments/files/2/a.png"]
        );
    }

    #[test]
    fn a_figure_of_the_page_is_a_whole_number_or_nothing() {
        assert_eq!(whole("40"), 40);
        assert_eq!(whole(" 10485760 "), 10_485_760);
        assert_eq!(whole("4.0"), 0);
        assert_eq!(whole(""), 0);
    }
}
