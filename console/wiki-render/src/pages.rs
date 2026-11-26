//! The core Markdown renderer as the plugin's host function: `markdown.pages`
//! (tehas-factory#3413, spec revision 10, section 5). The host calls
//! [`pages`] when a plugin asks for it, and nothing else of this crate: the
//! function is [`render::body`] and [`render::page`] with its escape pass, and
//! no other option. It is a pure string transform, so it has no grant, like
//! the crypto row.
//!
//! The caps are [`check`]'s, per call, and a call past one of them fails
//! whole: no partial answer, and the reason names the cap that fired, with
//! the size that crossed it. The answer counts toward the run's output caps
//! where the operator keeps them, not here.

use crate::check::{MAX_PAGE, MAX_PAGES, MAX_TOTAL};
use crate::render::{self, Fill};

/// One page of a call: what the plugin gives, and what the layout is filled
/// with. `markdown` is the page's own text, `title`, `build` and `up` go to
/// the layout escaped as they are, and `verified` is the word the layout gets.
#[derive(Clone, Debug)]
pub struct Page<'a> {
    pub markdown: &'a str,
    pub title: &'a str,
    pub build: &'a str,
    pub verified: bool,
    pub up: &'a str,
}

/// The word a verified page's layout holds, and nothing for one that is not.
const VERIFIED: &str = "verified";

/// Why a call is refused. Each reason names the cap or the field that fired,
/// as the spec's host functions refuse by name: one that reads the same for
/// two causes tells nothing about which check the call reached.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Refused {
    /// The layout is empty: there is nothing to fill.
    EmptyLayout,
    /// The call carries more pages than a call may.
    TooManyPages {
        /// What the call asked for.
        given: usize,
        /// The cap, in pages.
        cap: usize,
    },
    /// One page's Markdown is more than a page may be.
    PageTooLarge {
        /// Which page, counted from 0.
        at: usize,
        /// Its Markdown's size, in bytes.
        bytes: usize,
        /// The cap, in bytes.
        cap: usize,
    },
    /// The call in all is more than a call may be, counted over the layout
    /// and each page's fields.
    CallTooLarge {
        /// The size in all, in bytes.
        bytes: usize,
        /// The cap, in bytes.
        cap: usize,
    },
}

impl Refused {
    /// The reason a run sees: the name of the check that fired, and the size
    /// that crossed it.
    pub fn reason(&self) -> String {
        match self {
            Refused::EmptyLayout => "markdown.pages: the layout is empty".to_string(),
            Refused::TooManyPages { given, cap } => {
                format!("markdown.pages: {given} pages, over the cap of {cap} pages a call")
            }
            Refused::PageTooLarge { at, bytes, cap } => format!(
                "markdown.pages: page {at} is {bytes} bytes, over the cap of {cap} bytes a page"
            ),
            Refused::CallTooLarge { bytes, cap } => {
                format!("markdown.pages: {bytes} bytes in all, over the cap of {cap} bytes a call")
            }
        }
    }
}

/// The whole call: one HTML string a page, in order, or the one reason the
/// call is refused. Every cap is checked over all the pages before any page
/// is rendered, so a refused call renders nothing.
pub fn pages(layout: &str, given: &[Page]) -> Result<Vec<String>, Refused> {
    if layout.is_empty() {
        return Err(Refused::EmptyLayout);
    }
    if given.len() > MAX_PAGES {
        return Err(Refused::TooManyPages {
            given: given.len(),
            cap: MAX_PAGES,
        });
    }
    // The total counts what the call carries: the layout and every field of
    // every page, the boolean as the word the layout gets.
    let mut total = layout.len();
    for (at, p) in given.iter().enumerate() {
        if p.markdown.len() > MAX_PAGE {
            return Err(Refused::PageTooLarge {
                at,
                bytes: p.markdown.len(),
                cap: MAX_PAGE,
            });
        }
        total += p.markdown.len() + p.title.len() + p.build.len() + p.up.len();
        if p.verified {
            total += VERIFIED.len();
        }
    }
    if total > MAX_TOTAL {
        return Err(Refused::CallTooLarge {
            bytes: total,
            cap: MAX_TOTAL,
        });
    }
    Ok(given
        .iter()
        .map(|p| {
            let body = render::body(p.markdown);
            render::page(
                layout,
                &Fill {
                    title: p.title,
                    body: &body,
                    build: p.build,
                    verified: if p.verified { VERIFIED } else { "" },
                    up: p.up,
                },
            )
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn page<'a>(markdown: &'a str, title: &'a str) -> Page<'a> {
        Page {
            markdown,
            title,
            build: "2026-11-26-68ffab1d",
            verified: true,
            up: "",
        }
    }

    const LAYOUT: &str = "<title>{{title}}</title>{{verified}}<main>{{body}}</main>";

    /// What a cap's reason must say: its own words. A test asserts on these,
    /// so deleting the check it names cannot leave the test green: the call
    /// then answers, and no reason names anything.
    const PAGES_CAP_WORDS: &str = "cap of 500 pages";
    const PAGE_CAP_WORDS: &str = "cap of 65536 bytes a page";
    const TOTAL_CAP_WORDS: &str = "cap of 8388608 bytes a call";

    /// A page of `bytes` bytes of Markdown: whole words, so every byte counts.
    fn big(markdown_bytes: usize) -> String {
        let unit = "abcdefgh\n"; // 9 bytes
        let mut s = String::new();
        while s.len() < markdown_bytes {
            s.push_str(unit);
        }
        s.truncate(markdown_bytes);
        s
    }

    #[test]
    fn the_answer_is_one_string_a_page_in_order() {
        let out = pages(
            LAYOUT,
            &[
                page("# first\n", "First"),
                page("second *page*\n", "<i>Two</i>"),
            ],
        )
        .expect("a call of two ordinary pages is not refused");
        assert_eq!(out.len(), 2);
        assert!(
            out[0].starts_with("<title>First</title>verified<main><h1>first</h1>"),
            "{}",
            out[0]
        );
        assert!(
            out[1].contains("<title>&lt;i&gt;Two&lt;/i&gt;</title>"),
            "{}",
            out[1]
        );
        assert!(out[1].contains("<em>page</em>"), "{}", out[1]);
        assert!(out[0].find("first").unwrap() < out[1].find("second").unwrap());
    }

    #[test]
    fn a_page_that_is_not_verified_gets_no_word() {
        let mut p = page("x\n", "T");
        p.verified = false;
        let out = pages(LAYOUT, &[p]).expect("one ordinary page");
        assert!(out[0].contains("</title><main>"), "{}", out[0]);
    }

    #[test]
    fn five_hundred_pages_fit_and_the_next_page_names_its_cap() {
        let ok: Vec<Page> = (0..MAX_PAGES).map(|_| page("x\n", "T")).collect();
        assert_eq!(pages(LAYOUT, &ok).map(|v| v.len()), Ok(MAX_PAGES));
        let over = pages(LAYOUT, &[&ok[..], &[page("over\n", "Over")][..]].concat());
        let why = over.expect_err("the 501st page is refused").reason();
        assert!(why.contains(PAGES_CAP_WORDS), "{why}");
        assert!(why.contains("501"), "{why}");
        // The other caps did not fire: this is the pages check, reached.
        assert!(
            !why.contains(PAGE_CAP_WORDS) && !why.contains(TOTAL_CAP_WORDS),
            "{why}"
        );
    }

    #[test]
    fn a_page_at_64_kib_fits_and_one_more_byte_names_its_cap() {
        let ok = pages(LAYOUT, &[page(&big(MAX_PAGE), "T")]).expect("64 KiB fits");
        assert_eq!(ok.len(), 1);
        let why = pages(LAYOUT, &[page(&big(MAX_PAGE + 1), "T")])
            .expect_err("64 KiB plus a byte is refused")
            .reason();
        assert!(why.contains(PAGE_CAP_WORDS), "{why}");
        assert!(why.contains("65537"), "{why}");
        assert!(why.contains("page 0"), "{why}");
        assert!(
            !why.contains(PAGES_CAP_WORDS) && !why.contains(TOTAL_CAP_WORDS),
            "{why}: not the pages check"
        );
    }

    #[test]
    fn a_call_past_8_mib_names_its_cap_and_not_the_page_cap() {
        // Each page stays under the page cap, so only the total can refuse.
        let each = big(MAX_PAGE);
        let n = MAX_TOTAL / MAX_PAGE + 2;
        let call: Vec<Page> = (0..n).map(|_| page(each.as_str(), "T")).collect();
        let why = pages(LAYOUT, &call)
            .expect_err("the total is over 8 MiB")
            .reason();
        assert!(why.contains(TOTAL_CAP_WORDS), "{why}");
        assert!(
            !why.contains(PAGE_CAP_WORDS) && !why.contains(PAGES_CAP_WORDS),
            "{why}: the total check fired, not a page or a count"
        );
        // The same number of pages fits when each page is small, so the
        // refusal is the size and not the count.
        let small: Vec<Page> = (0..n).map(|_| page("x\n", "T")).collect();
        assert_eq!(pages(LAYOUT, &small).map(|v| v.len()), Ok(n));
    }

    #[test]
    fn the_layout_counts_toward_the_total() {
        // Pages well under every page cap, and few enough to pass the count:
        // only the layout can carry the call past 8 MiB.
        let layout = "y".repeat(MAX_TOTAL);
        let call = [page("x\n", "T"), page("y\n", "T")];
        let why = pages(&layout, &call)
            .expect_err("the layout alone passes the total")
            .reason();
        assert!(why.contains(TOTAL_CAP_WORDS), "{why}");
        assert!(why.contains(&MAX_TOTAL.to_string()), "{why}");
    }

    #[test]
    fn an_empty_layout_fails_the_call() {
        let why = pages("", &[page("x\n", "T")])
            .expect_err("an empty layout is refused")
            .reason();
        assert!(why.contains("the layout is empty"), "{why}");
    }

    #[test]
    fn a_refused_call_renders_nothing() {
        // Fail whole: one page past the page cap and no page is answered,
        // not even the ones that would have fit.
        assert!(pages(
            LAYOUT,
            &[page("fine\n", "A"), page(&big(MAX_PAGE + 1), "B")]
        )
        .is_err());
    }
}
