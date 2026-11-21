//! A page of a build as HTML: the Markdown the model wrote, with any HTML in
//! it shown as text, links between pages pointed at the HTML, images drawn as
//! their text, and one fixed layout around it. Pure.

use pulldown_cmark::{html, CowStr, Event, Options, Parser, Tag};

/// The Markdown the wiki is written in: CommonMark with tables and strikethrough.
pub const OPTIONS: Options = Options::ENABLE_TABLES.union(Options::ENABLE_STRIKETHROUGH);

/// The HTML file of a page: `modules/dispatch.md` is `modules/dispatch.html`.
pub fn html_name(path: &str) -> String {
    match path.strip_suffix(".md") {
        Some(stem) => format!("{stem}.html"),
        None => path.to_string(),
    }
}

/// A link's address in the HTML: a page of the wiki by its HTML file, an https
/// address as it is. The check has refused anything else before this runs.
fn address(link: &str) -> String {
    if link.starts_with("https://") || link.starts_with('#') {
        return link.to_string();
    }
    match link.split_once('#') {
        Some((path, part)) => format!("{}#{part}", html_name(path)),
        None => html_name(link),
    }
}

/// The body of a page, as HTML.
pub fn body(markdown: &str) -> String {
    let events = Parser::new_ext(markdown, OPTIONS).filter_map(|e| match e {
        Event::Html(t) | Event::InlineHtml(t) => Some(Event::Text(t)),
        Event::Start(Tag::Link {
            link_type,
            dest_url,
            title,
            id,
        }) => Some(Event::Start(Tag::Link {
            link_type,
            dest_url: CowStr::from(address(&dest_url)),
            title,
            id,
        })),
        Event::Start(Tag::Image { .. }) | Event::End(pulldown_cmark::TagEnd::Image) => None,
        other => Some(other),
    });
    let mut out = String::new();
    html::push_html(&mut out, events);
    out
}

/// Text for HTML: the five characters that mean something there, escaped.
pub fn escape(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars() {
        match c {
            '&' => out.push_str("&amp;"),
            '<' => out.push_str("&lt;"),
            '>' => out.push_str("&gt;"),
            '"' => out.push_str("&quot;"),
            '\'' => out.push_str("&#39;"),
            c => out.push(c),
        }
    }
    out
}

/// What the layout is filled with.
pub struct Fill<'a> {
    pub title: &'a str,
    pub body: &'a str,
    pub build: &'a str,
    pub verified: &'a str,
    /// The way from the page up to the build's own directory: `` or `../`.
    pub up: &'a str,
}

/// The whole page: the layout with each `{{name}}` filled, in one pass, so a
/// value is never read as a name. Every value but the body is text and
/// escaped; the body is what [`body`] made. An unknown name is left as it is.
pub fn page(layout: &str, f: &Fill) -> String {
    let mut out = String::with_capacity(layout.len() + f.body.len());
    let mut rest = layout;
    while let Some(start) = rest.find("{{") {
        out.push_str(&rest[..start]);
        let after = &rest[start + 2..];
        let Some(end) = after.find("}}") else {
            out.push_str(&rest[start..]);
            return out;
        };
        match &after[..end] {
            "title" => out.push_str(&escape(f.title)),
            "build" => out.push_str(&escape(f.build)),
            "verified" => out.push_str(&escape(f.verified)),
            "up" => out.push_str(&escape(f.up)),
            "body" => out.push_str(f.body),
            other => {
                out.push_str("{{");
                out.push_str(other);
                out.push_str("}}");
            }
        }
        rest = &after[end + 2..];
    }
    out.push_str(rest);
    out
}

/// The way up from a page to its build's directory.
pub fn up(path: &str) -> String {
    "../".repeat(path.matches('/').count())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn html_in_a_page_is_text() {
        let out = body("<script>alert(1)</script>\n\nA <b>bold</b> word.\n");
        assert!(!out.contains("<script"), "{out}");
        assert!(!out.contains("<b>"), "{out}");
        assert!(out.contains("&lt;script&gt;"), "{out}");
    }

    #[test]
    fn links_between_pages_point_at_the_html() {
        let out =
            body("[d](modules/dispatch.md) [p](a.md#part) [w](https://example.org/x) [h](#top)\n");
        for want in [
            "href=\"modules/dispatch.html\"",
            "href=\"a.html#part\"",
            "href=\"https://example.org/x\"",
            "href=\"#top\"",
        ] {
            assert!(out.contains(want), "{want}: {out}");
        }
    }

    #[test]
    fn an_image_is_its_text() {
        let out = body("![a diagram](https://example.org/x.png)\n");
        assert!(!out.contains("<img"), "{out}");
        assert!(out.contains("a diagram"), "{out}");
    }

    #[test]
    fn tables_render() {
        assert!(body("| a | b |\n| --- | --- |\n| 1 | 2 |\n").contains("<table>"));
    }

    #[test]
    fn the_layout_escapes_all_but_the_body() {
        let f = Fill {
            title: "<i>T</i> & \"q\"",
            body: "<p>b</p>",
            build: "2026-11-21-68ffab1d",
            verified: "68ffab1d",
            up: "../",
        };
        let out = page("<title>{{title}}</title><a href=\"{{up}}index.html\">{{build}} {{verified}}</a>{{body}}", &f);
        assert_eq!(
            out,
            "<title>&lt;i&gt;T&lt;/i&gt; &amp; &quot;q&quot;</title><a href=\"../index.html\">2026-11-21-68ffab1d 68ffab1d</a><p>b</p>"
        );
        let f = Fill {
            title: "{{body}}",
            ..f
        };
        assert_eq!(
            page("{{title}}|{{body}}|{{other}}", &f),
            "{{body}}|<p>b</p>|{{other}}"
        );
        assert_eq!(up("index.md"), "");
        assert_eq!(up("a/b/c.md"), "../../");
        assert_eq!(html_name("a/b.md"), "a/b.html");
    }
}
