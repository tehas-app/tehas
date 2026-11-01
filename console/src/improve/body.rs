//! The text of an issue and of a comment. The body is the loop's: the model's
//! text goes in with every mention broken and every HTML comment shown, and its
//! evidence inside a fence it cannot close.

use super::check::{Finding, Recheck};
use crate::jq::first;
use std::collections::BTreeSet;

pub fn defuse(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        out.push(c);
        if c == '@' && chars.peek().is_some_and(char::is_ascii_alphanumeric) {
            out.push('\u{200b}');
        }
    }
    out.replace("<!--", "&lt;!--")
}

/// The evidence with no run of backticks that could close its fence.
fn unfenced(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    let mut rest = s;
    while let Some(at) = rest.find('`') {
        out.push_str(&rest[..at]);
        let after = rest[at..].trim_start_matches('`');
        let run = rest.len() - at - after.len();
        if run >= 3 {
            out.push_str("'''");
        } else {
            out.push_str(&"`".repeat(run));
        }
        rest = after;
    }
    out.push_str(rest);
    out
}

fn sorted(v: impl Iterator<Item = String>) -> String {
    v.collect::<BTreeSet<String>>()
        .into_iter()
        .collect::<Vec<String>>()
        .join(", ")
}

fn lines(given: &[&str]) -> String {
    given.iter().map(|l| format!("{l}\n")).collect()
}

/// What an issue says beside the finding.
pub struct Page<'a> {
    pub model: &'a str,
    pub product: &'a str,
    pub links: &'a str,
    pub does: &'a str,
    /// both, factory, rukas, or nothing for a finding that has no halves.
    pub half: &'a str,
    pub other_repo: &'a str,
    pub other: &'a str,
    pub others: &'a str,
}

pub fn issue(f: &Finding, p: &Page) -> String {
    let body = defuse(&f.body);
    let factory = defuse(&f.factory_part);
    let rukas = defuse(&f.rukas_part);
    let mut out = lines(&[
        &format!(
            "_Filed by the tehas improvement loop, read by the model `{}`. It is a proposal: nothing works on it until a person labels it._",
            p.model
        ),
        "",
        &format!("**Category:** {}", f.category),
        "",
        &format!("**Working on it:** {}", p.does),
    ]);
    // Where else the fix lives, when the finding speaks of a repository of the
    // stack beside the one this issue is filed in. A person reads it before
    // labelling; the dispatcher holds a cycle back on the label that goes with
    // it.
    if !p.others.is_empty() {
        out += &lines(&["", &format!("**Names:** {}", p.others)]);
    }
    let issues = sorted(f.cycles.iter().map(|c| {
        let number = c.split('-').next().unwrap_or("");
        format!("{}#{number}", p.product)
    }));
    out += &lines(&["", &format!("**Issues:** {issues}"), "", &body, ""]);
    if f.category == "boundary" {
        if p.half == "both" {
            out += &lines(&[
                "**The factory's half**",
                "",
                &factory,
                "",
                "**The half for pi-rukas**",
                "",
                &rukas,
                "",
            ]);
        } else {
            let (own, theirs) = if p.half == "factory" {
                (&factory, &rukas)
            } else {
                (&rukas, &factory)
            };
            let link = if p.other.is_empty() {
                String::new()
            } else {
                format!(": {}", p.other)
            };
            out += &lines(&[
                "**This repository's half**",
                "",
                own,
                "",
                &format!("**The other half**, filed in {}{link}", p.other_repo),
                "",
                theirs,
                "",
            ]);
        }
    }
    let cycles = sorted(f.cycles.iter().cloned());
    out += &lines(&[
        "**Evidence**",
        "",
        "````text",
        &unfenced(&f.evidence),
        "````",
        "",
        &format!("**Cycles:** {cycles}"),
        "",
        "**Whole logs**",
        "",
        p.links.strip_suffix('\n').unwrap_or(p.links),
    ]);
    out
}

/// What a recheck comment says beside the analyst's note. `fix` is empty for
/// an issue no commit closed; `in_build` is yes, no or unknown for one that was.
pub struct Said<'a> {
    pub model: &'a str,
    pub product: &'a str,
    pub links: &'a str,
    pub rukas: &'a str,
    pub image: &'a str,
    pub fix: &'a str,
    pub build: &'a str,
    pub in_build: &'a str,
}

fn seen_again(r: &Recheck, s: &Said) -> String {
    let said = if r.seen { "yes" } else { "no" };
    let (f, b) = (first(s.fix, 12), first(s.build, 12));
    if s.fix.is_empty() {
        said.to_string()
    } else if s.in_build == "yes" {
        format!("{said}, with the fix `{f}` in this build")
    } else if s.in_build == "no" && r.seen {
        format!("not in this build: the fix `{f}` is not in `{b}`, so this cycle ran without it")
    } else if s.in_build == "no" {
        format!("no, though the fix `{f}` is not in `{b}`")
    } else if s.build.is_empty() {
        format!(
            "{said}, build unknown: the record names no commit of {} that this cycle ran, so whether the fix `{f}` was in it is not known",
            r.repo
        )
    } else {
        format!("{said}, build unknown: GitHub did not say whether the fix `{f}` is in `{b}`")
    }
}

pub fn comment(r: &Recheck, s: &Said) -> String {
    let number = r.cycle.split('-').next().unwrap_or("");
    let at = if s.rukas.is_empty() {
        "a commit the record does not name".to_string()
    } else {
        format!("`{}`", s.rukas)
    };
    let image = if s.image.is_empty() {
        String::new()
    } else {
        format!(", image `{}`", s.image)
    };
    lines(&[
        &format!(
            "_The tehas improvement loop, read by the model `{}`. A model wrote this: check it against the log._",
            s.model
        ),
        "",
        &format!(
            "{}#{number} was worked on again: cycle `{}`, with pi-rukas at {at}{image}.",
            s.product, r.cycle
        ),
        "",
        &format!("**Seen again:** {}", seen_again(r, s)),
        "",
        &defuse(&r.note),
        "",
        "**Whole log**",
        "",
        s.links.strip_suffix('\n').unwrap_or(s.links),
    ])
}

#[cfg(test)]
mod tests {
    use super::*;

    fn finding(category: &str) -> Finding {
        Finding {
            title: "T".into(),
            category: category.into(),
            body: "Ask @alice. <!-- x -->\n\n## Done when\n\n- a".into(),
            evidence: "a ``` b ````` c `` d".into(),
            cycles: vec!["9-261010-210000".into(), "7-261010-200140".into()],
            factory_part: "F @b".into(),
            rukas_part: "R".into(),
            others: vec![],
        }
    }

    fn page<'a>(half: &'a str, other: &'a str, others: &'a str) -> Page<'a> {
        Page {
            model: "m",
            product: "a/w",
            links: "- [l](u)\n",
            does: "Does.",
            half,
            other_repo: "a/r",
            other,
            others,
        }
    }

    #[test]
    fn a_mention_is_broken_and_a_comment_is_shown() {
        assert_eq!(
            defuse("@alice @ a@b @-x @@c"),
            "@\u{200b}alice @ a@\u{200b}b @-x @@\u{200b}c"
        );
        assert_eq!(defuse("<!-- x --> <!--"), "&lt;!-- x --> &lt;!--");
        assert_eq!(defuse("@é"), "@é");
        assert_eq!(defuse("end@"), "end@");
    }

    #[test]
    fn evidence_cannot_close_its_fence() {
        assert_eq!(
            unfenced("a ``` b ```` c `` d ` e"),
            "a ''' b ''' c `` d ` e"
        );
        assert_eq!(unfenced("`````"), "'''");
        assert_eq!(unfenced("plain"), "plain");
    }

    #[test]
    fn an_issue_is_written_line_by_line() {
        let got = issue(&finding("cycle"), &page("", "", ""));
        assert_eq!(
            got,
            "_Filed by the tehas improvement loop, read by the model `m`. It is a proposal: nothing works on it until a person labels it._\n\
             \n**Category:** cycle\n\n**Working on it:** Does.\n\n\
             **Issues:** a/w#7, a/w#9\n\n\
             Ask @\u{200b}alice. &lt;!-- x -->\n\n## Done when\n\n- a\n\n\
             **Evidence**\n\n````text\na ''' b ''' c `` d\n````\n\n\
             **Cycles:** 7-261010-200140, 9-261010-210000\n\n**Whole logs**\n\n- [l](u)\n"
        );
    }

    #[test]
    fn names_stand_above_the_issues() {
        let got = issue(&finding("cycle"), &page("", "", "a/r, a/s"));
        assert!(got.contains("Does.\n\n**Names:** a/r, a/s\n\n**Issues:**"));
    }

    #[test]
    fn the_halves_of_a_boundary_finding() {
        let both = issue(&finding("boundary"), &page("both", "", ""));
        assert!(both.contains(
            "- a\n\n**The factory's half**\n\nF @\u{200b}b\n\n**The half for pi-rukas**\n\nR\n\n**Evidence**"
        ));
        let factory = issue(&finding("boundary"), &page("factory", "", ""));
        assert!(factory.contains(
            "**This repository's half**\n\nF @\u{200b}b\n\n**The other half**, filed in a/r\n\nR\n\n"
        ));
        let rukas = issue(&finding("boundary"), &page("rukas", "https://x/1", ""));
        assert!(rukas.contains(
            "**This repository's half**\n\nR\n\n**The other half**, filed in a/r: https://x/1\n\nF @\u{200b}b\n\n"
        ));
        let stray = issue(&finding("cycle"), &page("both", "", ""));
        assert!(!stray.contains("half"));
    }

    fn recheck(seen: bool) -> Recheck {
        Recheck {
            repo: "a/r".into(),
            number: "12".into(),
            cycle: "7-261010-200140".into(),
            seen,
            note: "thanks @alice <!-- x".into(),
        }
    }

    const FIX: &str = "abcdefabcdefabcdefabcdefabcdefabcdefabcd";
    const BUILD: &str = "0123456789abcdef0123456789abcdef01234567";

    fn said<'a>(fix: &'a str, build: &'a str, in_build: &'a str) -> Said<'a> {
        Said {
            model: "m",
            product: "a/w",
            links: "- [l](u)",
            rukas: "",
            image: "",
            fix,
            build,
            in_build,
        }
    }

    #[test]
    fn a_comment_is_written_line_by_line() {
        assert_eq!(
            comment(&recheck(false), &said("", "", "")),
            "_The tehas improvement loop, read by the model `m`. A model wrote this: check it against the log._\n\
             \na/w#7 was worked on again: cycle `7-261010-200140`, with pi-rukas at a commit the record does not name.\n\
             \n**Seen again:** no\n\nthanks @\u{200b}alice &lt;!-- x\n\n**Whole log**\n\n- [l](u)\n"
        );
        let mut s = said("", "", "");
        s.rukas = BUILD;
        s.image = "sha256:e";
        assert!(comment(&recheck(true), &s).contains(&format!(
            "with pi-rukas at `{BUILD}`, image `sha256:e`.\n\n**Seen again:** yes\n"
        )));
    }

    #[test]
    fn seen_again_says_what_is_known_of_the_build() {
        let line =
            |seen, fix, build, in_build| seen_again(&recheck(seen), &said(fix, build, in_build));
        assert_eq!(line(true, "", "", ""), "yes");
        assert_eq!(line(false, "", "", ""), "no");
        assert_eq!(
            line(true, FIX, BUILD, "yes"),
            "yes, with the fix `abcdefabcdef` in this build"
        );
        assert_eq!(
            line(false, FIX, BUILD, "yes"),
            "no, with the fix `abcdefabcdef` in this build"
        );
        assert_eq!(
            line(true, FIX, BUILD, "no"),
            "not in this build: the fix `abcdefabcdef` is not in `0123456789ab`, so this cycle ran without it"
        );
        assert_eq!(
            line(false, FIX, BUILD, "no"),
            "no, though the fix `abcdefabcdef` is not in `0123456789ab`"
        );
        assert_eq!(
            line(true, FIX, "", "unknown"),
            "yes, build unknown: the record names no commit of a/r that this cycle ran, so whether the fix `abcdefabcdef` was in it is not known"
        );
        assert_eq!(
            line(false, FIX, BUILD, "unknown"),
            "no, build unknown: GitHub did not say whether the fix `abcdefabcdef` is in `0123456789ab`"
        );
    }
}
