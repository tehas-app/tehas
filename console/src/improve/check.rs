//! What is filed. One pass over the file the analyst leaves decides it. A
//! finding that is not the shape asked for is dropped; a file that is not is an
//! error. A title is compared in lower case without punctuation, with the open
//! issues and with the findings before it.
//!
//! A body without a Done-when section is the wrong shape too: a cycle that is
//! given a proposal with no bar to check itself against spends its rounds
//! inventing one. The model's body is untrusted, so the section has to hold up
//! when read as markdown: text in a code fence or an HTML comment is not a
//! heading and not a bar, and the section is the last thing in the body.
//!
//! A recheck is taken only for an issue this loop filed, named by a cycle of
//! the same product issue as the new one: the model cannot have a comment put
//! anywhere else.
//!
//! The rules were a jq program, and they keep its semantics: a length is in
//! codepoints, 5.0 equals 5, and `unique` sorts by bytes.

use crate::jq::{first, obj, J};
use std::cmp::Ordering;
use std::collections::{BTreeMap, BTreeSet};

const CATEGORIES: [&str; 7] = [
    "dispatcher",
    "cycle",
    "pi-rukas",
    "console",
    "gate",
    "docs",
    "boundary",
];
const MAX_RUN: usize = 3;
const MAX_RECHECK: usize = 10;
// Tab, line feed and carriage return.
const TEXT: [char; 3] = ['\t', '\n', '\r'];

/// A finding that will be filed.
#[derive(Debug)]
pub struct Finding {
    pub title: String,
    pub category: String,
    pub body: String,
    pub evidence: String,
    pub cycles: Vec<String>,
    pub factory_part: String,
    pub rukas_part: String,
    /// The stack repositories it speaks of. Beside the finding and never read
    /// from it: the loop reads the stack, and what it read is what a person
    /// can trust.
    pub others: Vec<String>,
}

impl Finding {
    /// The stack repositories it speaks of, none of them `to`.
    pub fn others_than(&self, to: &str) -> Vec<&str> {
        self.others
            .iter()
            .map(String::as_str)
            .filter(|r| *r != to)
            .collect()
    }
}

/// A comment on an issue worked on again.
#[derive(Debug)]
pub struct Recheck {
    pub repo: String,
    /// As the file writes it.
    pub number: String,
    pub cycle: String,
    pub seen: bool,
    pub note: String,
}

#[derive(Debug)]
pub struct Plan {
    pub found: usize,
    pub invalid: usize,
    pub skipped: usize,
    pub duplicates: usize,
    pub of: String,
    pub over: usize,
    pub file: Vec<Finding>,
    pub recheck: Vec<Recheck>,
}

/// What the file is checked against.
pub struct Given<'a> {
    pub cycles: &'a [String],
    pub stack: &'a [J],
    pub rooms: &'a BTreeMap<String, i64>,
    pub repo: &'a str,
    pub rukas: &'a str,
    pub label: &'a str,
    pub skip_rukas: &'a str,
}

/// `<issue>-<stamp>`, the name of a record.
pub fn is_record(name: &str) -> bool {
    let digits = |s: &str| !s.is_empty() && s.bytes().all(|b| b.is_ascii_digit());
    let mut parts = name.split('-');
    let (Some(issue), Some(day), Some(time), None) =
        (parts.next(), parts.next(), parts.next(), parts.next())
    else {
        return false;
    };
    digits(issue) && digits(day) && digits(time) && day.len() == 6 && time.len() == 6
}

/// The cycles an issue's body names on its Cycles lines. The loop writes those
/// lines, so what is not the name of a record is not read.
fn cycles_of(body: &str) -> Vec<J> {
    const MARK: &str = "**Cycles:** ";
    let mut out = Vec::new();
    let mut rest = body;
    while let Some(at) = rest.find(MARK) {
        rest = &rest[at + MARK.len()..];
        let line = rest.split('\n').next().unwrap_or("");
        rest = &rest[line.len()..];
        out.extend(line.split(", ").filter(|c| is_record(c)).map(J::str));
    }
    out
}

/// One repository's part of the stack, from what `gh issue list` gave: each
/// list's issues with the repository beside them, the cycles an issue rests
/// on, and its body cut to 4000 characters. None for what is not such a list.
pub fn issues_of(repo: &str, lists: &[&str]) -> Option<Vec<J>> {
    let mut out = Vec::new();
    for list in lists {
        let J::Arr(issues) = J::parse(list)? else {
            return None;
        };
        for i in &issues {
            let J::Arr(labels) = i.get("labels") else {
                return None;
            };
            let names = labels
                .iter()
                .map(|l| match l {
                    J::Obj(_) | J::Null => Some(l.get("name").clone()),
                    _ => None,
                })
                .collect::<Option<Vec<J>>>()?;
            let body = match i.get("body").or(&J::Null) {
                J::Null | J::Bool(false) => "",
                J::Str(s) => s,
                _ => return None,
            };
            out.push(obj(vec![
                ("repo", J::str(repo)),
                ("number", i.get("number").clone()),
                ("title", i.get("title").clone()),
                ("state", i.get("state").clone()),
                ("labels", J::Arr(names)),
                ("cycles", J::Arr(cycles_of(body))),
                ("body", J::Str(first(body, 4000))),
            ]));
        }
    }
    Some(out)
}

/// What each repository can still take. One that is full takes no more, and a
/// run waits only when every one is.
pub fn rooms(stack: &[J], repos: &[String], label: &str, max: i64) -> BTreeMap<String, i64> {
    repos
        .iter()
        .map(|r| {
            let open = stack
                .iter()
                .filter(|i| {
                    i.get("repo").as_str() == Some(r)
                        && i.get("state").as_str() == Some("OPEN")
                        && has(i.get("labels"), label)
                })
                .count();
            (r.clone(), (max - open as i64).max(0))
        })
        .collect()
}

fn has(list: &J, word: &str) -> bool {
    list.items().iter().any(|l| l.as_str() == Some(word))
}

/// jq's `==` of two numbers.
fn same_number(a: &J, b: &J) -> bool {
    matches!((a, b), (J::Num(_), J::Num(_))) && a.as_f64() == b.as_f64()
}

fn clean(s: &str, allow: &[char]) -> bool {
    s.chars()
        .all(|c| (c as u32 >= 32 && c as u32 != 127) || allow.contains(&c))
}

fn norm(title: &str) -> String {
    let kept: String = title
        .to_ascii_lowercase()
        .chars()
        .filter(|c| matches!(c, 'a'..='z' | '0'..='9' | ' '))
        .collect();
    let words: Vec<&str> = kept.split(' ').filter(|w| !w.is_empty()).collect();
    if words.is_empty() {
        title.to_string()
    } else {
        words.join(" ")
    }
}

/// Nothing a person can read: spaces, tabs, and the blanks a copy-pasted line
/// or a model can leave behind that no one can see.
fn blank(line: &str) -> bool {
    line.chars().all(|c| {
        matches!(
            c,
            ' ' | '\t' | '\u{a0}' | '\u{2000}'..='\u{200f}' | '\u{2800}' | '\u{3000}'
        )
    })
}

fn indent(line: &str) -> &str {
    line.trim_start_matches([' ', '\t'])
}

/// The rest after one space or more.
fn spaced(s: &str) -> Option<&str> {
    s.starts_with(' ').then(|| s.trim_start_matches(' '))
}

/// The line that is "## Done when" and nothing else but spaces around it.
fn heading(line: &str) -> bool {
    let words = || {
        let s = spaced(indent(line).strip_prefix("##")?)?;
        let s = spaced(s.strip_prefix("Done")?)?;
        s.strip_prefix("when")
    };
    words().is_some_and(|rest| rest.bytes().all(|b| b == b' '))
}

/// Three of either kind, with nothing before them but spaces. A fence holds
/// the text a model pasted in, so what it holds is not markdown.
fn fence(line: &str) -> bool {
    let s = indent(line);
    s.starts_with("```") || s.starts_with("~~~")
}

fn item(line: &str) -> bool {
    let s = indent(line);
    if s.starts_with("- ") || s.starts_with("* ") {
        return true;
    }
    let number = s.trim_start_matches(|c: char| c.is_ascii_digit());
    number.len() < s.len() && number.starts_with(". ")
}

/// The bar is real when one line is the heading, and every line a person can
/// read below it, at least one, is an item. Text after an HTML comment's start
/// is gone, and what a fence hides is not read.
pub fn done_when(body: &str) -> bool {
    let text = body.replace('\r', "");
    let mut fenced = false;
    let mut read: Vec<&str> = Vec::new();
    for line in text.split('\n') {
        let line = line.find("<!--").map_or(line, |at| &line[..at]);
        let edge = fence(line);
        if !fenced && !edge && !blank(line) {
            read.push(line);
        }
        fenced ^= edge;
    }
    let mut heads = read.iter().enumerate().filter(|(_, l)| heading(l));
    let (Some((at, _)), None) = (heads.next(), heads.next()) else {
        return false;
    };
    let below = &read[at + 1..];
    !below.is_empty() && below.iter().all(|l| item(l))
}

fn text(v: &J, least: usize, most: usize, allow: &[char]) -> bool {
    v.as_str()
        .is_some_and(|s| (least..=most).contains(&v.len()) && clean(s, allow))
}

fn valid(f: &J, g: &Given) -> bool {
    let J::Obj(_) = f else { return false };
    if !text(f.get("title"), 1, 100, &[]) {
        return false;
    }
    let category = f.get("category").as_str().unwrap_or("");
    if !CATEGORIES.contains(&category) {
        return false;
    }
    if category == "boundary"
        && !["factory_part", "rukas_part"]
            .iter()
            .all(|k| text(f.get(k), 1, 3000, &TEXT))
    {
        return false;
    }
    if !text(f.get("body"), 1, 6000, &TEXT) || !done_when(f.get("body").as_str().unwrap_or("")) {
        return false;
    }
    if !text(f.get("evidence"), 0, 3000, &TEXT) {
        return false;
    }
    let J::Arr(cycles) = f.get("cycles") else {
        return false;
    };
    let known = |c: &J| c.as_str().is_some_and(|s| g.cycles.iter().any(|k| k == s));
    if cycles.is_empty() || !cycles.iter().all(known) {
        return false;
    }
    let dup = f.get("duplicate_of");
    dup.is_null() || g.stack.iter().any(|i| same_number(i.get("number"), dup))
}

/// An optional text in a join: null is nothing, a number or a boolean its JSON,
/// and anything else ends the join, as it did jq's.
fn joined(v: &J) -> Option<String> {
    match v {
        J::Null => Some(String::new()),
        J::Str(s) => Some(s.clone()),
        J::Num(_) | J::Bool(_) => Some(v.dump()),
        _ => None,
    }
}

/// The stack repositories a finding speaks of: named as owner/name: anywhere in
/// its text, or named by an issue of the stack that cites one of its cycles.
/// That citation is the strongest tell there is, since the finding rests on a
/// cycle whose issue is open there, and the loop writes it itself on the Cycles
/// line, so a model cannot word a finding into it or out of it.
fn speaks(f: &J, repos: &BTreeSet<&str>, stack: &[J]) -> Option<Vec<String>> {
    let said = [
        joined(f.get("title"))?,
        joined(f.get("body"))?,
        joined(f.get("evidence"))?,
        joined(f.get("factory_part"))?,
        joined(f.get("rukas_part"))?,
    ]
    .join("\n");
    let cites = |repo: &str, cycle: &J| {
        stack.iter().any(|i| {
            i.get("repo").as_str() == Some(repo)
                && i.get("cycles")
                    .items()
                    .iter()
                    .any(|c| c.as_str().is_some() && c.as_str() == cycle.as_str())
        })
    };
    Some(
        repos
            .iter()
            .filter(|r| {
                said.contains(&format!("{r}:"))
                    || f.get("cycles").items().iter().any(|c| cites(r, c))
            })
            .map(|r| r.to_string())
            .collect(),
    )
}

fn rechecked(u: &J, g: &Given) -> bool {
    let J::Obj(_) = u else { return false };
    let (Some(repo), J::Num(_), Some(cycle), J::Bool(_)) = (
        u.get("repo").as_str(),
        u.get("number"),
        u.get("cycle").as_str(),
        u.get("seen"),
    ) else {
        return false;
    };
    if !g.cycles.iter().any(|c| c == cycle) || !text(u.get("note"), 1, 1500, &TEXT) {
        return false;
    }
    let issue = |c: &str| c.split('-').next().unwrap_or("").to_string();
    g.stack.iter().any(|i| {
        let cycles = i.get("cycles");
        i.get("repo").as_str() == Some(repo)
            && same_number(i.get("number"), u.get("number"))
            && has(i.get("labels"), g.label)
            && !has(cycles, cycle)
            && cycles
                .items()
                .iter()
                .filter_map(J::as_str)
                .any(|c| issue(c) == issue(cycle))
    })
}

fn to_finding(f: &J, others: Vec<String>) -> Finding {
    let s = |k: &str| f.get(k).as_str().unwrap_or("").to_string();
    Finding {
        title: s("title"),
        category: s("category"),
        body: s("body"),
        evidence: s("evidence"),
        cycles: f
            .get("cycles")
            .items()
            .iter()
            .filter_map(J::as_str)
            .map(str::to_string)
            .collect(),
        factory_part: s("factory_part"),
        rukas_part: s("rukas_part"),
        others,
    }
}

fn to_recheck(u: &J) -> Recheck {
    let s = |k: &str| u.get(k).as_str().unwrap_or("").to_string();
    Recheck {
        repo: s("repo"),
        number: u.get("number").text(),
        cycle: s("cycle"),
        seen: u.get("seen").is_true(),
        note: s("note"),
    }
}

// Sorted by repository, number and cycle, the first of each kept.
fn by_key(a: &J, b: &J) -> Ordering {
    let key = |v: &J, k: &str| v.get(k).clone();
    J::Arr(vec![key(a, "repo"), key(a, "number"), key(a, "cycle")]).cmp(&J::Arr(vec![
        key(b, "repo"),
        key(b, "number"),
        key(b, "cycle"),
    ]))
}

/// The plan of a run, or None for a file that is not the shape asked for.
pub fn plan(doc: &J, g: &Given) -> Option<Plan> {
    let (J::Obj(_), J::Arr(all)) = (doc, doc.get("findings")) else {
        return None;
    };
    let mut seen = Vec::new();
    for i in g.stack {
        if i.get("state").as_str() == Some("OPEN") {
            seen.push(norm(i.get("title").as_str()?));
        }
    }
    let good_shape: Vec<&J> = all.iter().filter(|f| valid(f, g)).collect();
    let good: Vec<&J> = good_shape
        .iter()
        .copied()
        .filter(|f| g.skip_rukas == "0" || f.get("category").as_str() != Some("pi-rukas"))
        .collect();
    let mut keep = Vec::new();
    for f in good.iter().filter(|f| f.get("duplicate_of").is_null()) {
        let t = norm(f.get("title").as_str().unwrap_or(""));
        if !seen.contains(&t) {
            seen.push(t);
            keep.push(*f);
        }
    }
    let mut room = g.rooms.clone();
    let mut left = MAX_RUN;
    let mut file = Vec::new();
    for f in &keep {
        let to: Vec<&str> = match f.get("category").as_str() {
            Some("pi-rukas") => vec![g.rukas],
            Some("boundary") if g.repo != g.rukas => vec![g.repo, g.rukas],
            _ => vec![g.repo],
        };
        if left >= to.len() && to.iter().all(|r| room.get(*r).is_some_and(|n| *n > 0)) {
            for r in &to {
                if let Some(n) = room.get_mut(*r) {
                    *n -= 1;
                }
            }
            left -= to.len();
            file.push(*f);
        }
    }
    // Every repository the loop files in is a stack repository, whether or not
    // an issue there was read. The one for pi-rukas is too where its findings
    // are not filed: the text of a finding names it all the same.
    let repos: BTreeSet<&str> = g
        .stack
        .iter()
        .filter_map(|i| i.get("repo").as_str())
        .chain([g.repo, g.rukas])
        .collect();
    let filed = file
        .iter()
        .map(|f| Some(to_finding(f, speaks(f, &repos, g.stack)?)))
        .collect::<Option<Vec<Finding>>>()?;
    let of: BTreeSet<String> = good
        .iter()
        .map(|f| f.get("duplicate_of"))
        .filter(|d| !d.is_null())
        .map(|d| format!("#{}", d.text()))
        .collect();
    let mut followups: Vec<&J> = doc
        .get("followups")
        .items()
        .iter()
        .filter(|u| rechecked(u, g))
        .collect();
    followups.sort_by(|a, b| by_key(a, b));
    followups.dedup_by(|a, b| by_key(a, b).is_eq());
    followups.truncate(MAX_RECHECK);
    Some(Plan {
        found: all.len(),
        invalid: all.len() - good_shape.len(),
        skipped: good_shape.len() - good.len(),
        duplicates: good.len() - keep.len(),
        of: of.into_iter().collect::<Vec<String>>().join(","),
        over: keep.len() - file.len(),
        file: filed,
        recheck: followups.into_iter().map(to_recheck).collect(),
    })
}

/// Every string a stopped analyst's file names as a cycle, valid or not: such a
/// record was read.
pub fn named(doc: &J) -> Vec<String> {
    let findings = doc
        .get("findings")
        .items()
        .iter()
        .filter(|f| matches!(f, J::Obj(_)))
        .flat_map(|f| f.get("cycles").items());
    let followups = doc
        .get("followups")
        .items()
        .iter()
        .filter(|u| matches!(u, J::Obj(_)))
        .map(|u| u.get("cycle"));
    findings
        .chain(followups)
        .filter_map(J::as_str)
        .flat_map(|s| s.split('\n'))
        .map(str::to_string)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    const REC: &str = "7-261010-200140";

    fn repo_of(r: &str) -> String {
        r.to_string()
    }

    struct World {
        cycles: Vec<String>,
        stack: Vec<J>,
        rooms: BTreeMap<String, i64>,
        rukas: String,
        skip: String,
    }

    fn world() -> World {
        World {
            cycles: vec![REC.to_string()],
            stack: Vec::new(),
            rooms: BTreeMap::from([(repo_of("a/f"), 10), (repo_of("a/r"), 10)]),
            rukas: "a/r".to_string(),
            skip: "0".to_string(),
        }
    }

    fn plan_of(doc: &str, w: &World) -> Option<Plan> {
        plan(
            &J::parse(doc).unwrap(),
            &Given {
                cycles: &w.cycles,
                stack: &w.stack,
                rooms: &w.rooms,
                repo: "a/f",
                rukas: &w.rukas,
                label: "tehas:improvement",
                skip_rukas: &w.skip,
            },
        )
    }

    const BODY: &str = "Why.\\n\\n## Done when\\n\\n- a test.";

    fn finding(title: &str, more: &str) -> String {
        format!(
            r#"{{"title":"{title}","category":"cycle","body":"{BODY}","evidence":"e","cycles":["{REC}"],"duplicate_of":null{more}}}"#
        )
    }

    fn findings(list: &[String]) -> String {
        format!(r#"{{"findings":[{}]}}"#, list.join(","))
    }

    fn issue(n: u32, title: &str, state: &str, labels: &[&str], cycles: &[&str]) -> J {
        let labels = labels.iter().map(|l| J::str(*l)).collect();
        let cycles = cycles.iter().map(|c| J::str(*c)).collect();
        obj(vec![
            ("repo", J::str("a/f")),
            ("number", J::num(n as f64)),
            ("title", J::str(title)),
            ("state", J::str(state)),
            ("labels", J::Arr(labels)),
            ("cycles", J::Arr(cycles)),
            ("body", J::str("")),
        ])
    }

    #[test]
    fn a_record_is_an_issue_and_a_stamp() {
        for good in ["7-261010-200140", "1234-000000-000000"] {
            assert!(is_record(good), "{good}");
        }
        for bad in [
            "",
            "-261010-200140",
            "7-26101-200140",
            "7-261010-2001400",
            "7-261010-200140-1",
            "7-261010",
            "x-261010-200140",
            "7-261010-200140\n",
            "٧-261010-200140",
        ] {
            assert!(!is_record(bad), "{bad}");
        }
    }

    #[test]
    fn control_characters_are_not_clean_but_the_allowed_ones_are() {
        assert!(clean("plain é \u{85} text", &[]));
        assert!(!clean("a\tb", &[]));
        assert!(clean("a\tb\nc\rd", &TEXT));
        assert!(!clean("a\u{7}b", &TEXT));
        assert!(!clean("a\u{7f}b", &TEXT));
        assert!(!clean("a\u{0}b", &TEXT));
    }

    #[test]
    fn a_title_is_compared_without_case_or_punctuation() {
        assert_eq!(norm("  The Gate, runs  TWICE! "), "the gate runs twice");
        assert_eq!(norm("Grüße"), "gre");
        assert_eq!(norm("a\nb"), "ab");
        assert_eq!(norm("!!!"), "!!!");
        assert_eq!(norm("  "), "  ");
    }

    #[test]
    fn the_bar_is_the_last_thing_and_holds_items() {
        for good in [
            "## Done when\n- a",
            "Why.\n\n## Done when\n\n- a\n* b\n3. c\n\n",
            "Why.\r\n\r\n  ##  Done   when  \r\n\t- a\r\n",
            "Why.\n\n## Done when\n\n- a\n\n```text\nmore\n```",
            "Why.\n\n## Done when\n\n- a\n\n\u{a0}\n\n\u{200b}\n   ",
            "## Done when\n- a <!-- and a comment",
            "## Done when\n<!-- only a comment\n- a",
        ] {
            assert!(done_when(good), "{good:?}");
        }
        for bad in [
            "",
            "\r",
            "Why.",
            "## Done when",
            "## Done when\n   \n\t\n",
            "## Done when, eventually\n- a",
            "### Done when\n- a",
            "## Done  when:\n- a",
            "##Done when\n- a",
            "## done when\n- a",
            "## Done when\n- a\nprose",
            "## Done when\n- a\n\n## Notes\n- b",
            "## Done when\n- a\n## Done when\n- b",
            "## Done when\nprose\n- a",
            "## Done when\n-a",
            "## Done when\n1.a",
            "## Done when\n.1 a",
            "```\n## Done when\n- a\n```\nWhy.",
            "~~~ text\n## Done when\n- a\n~~~",
            "<!-- ## Done when\n- a",
            "<!--\n## Done when\n- a\n-->",
            "## Done when\n\u{a0}\n\u{200b}",
            "```\n## Done when\n- a",
        ] {
            assert!(!done_when(bad), "{bad:?}");
        }
    }

    #[test]
    fn a_fence_of_one_kind_is_closed_by_the_other() {
        assert!(done_when("```\nhidden\n~~~\n## Done when\n- a"));
    }

    #[test]
    fn a_file_that_is_not_the_shape_asked_for_is_refused() {
        let w = world();
        assert!(plan_of(r#"["a list"]"#, &w).is_none());
        assert!(plan_of(r#"{"findings":"none"}"#, &w).is_none());
        assert!(plan_of("{}", &w).is_none());
        assert!(plan_of(r#"{"findings":[]}"#, &w).is_some());
    }

    #[test]
    fn a_length_is_in_codepoints() {
        let w = world();
        let at_most = "é".repeat(100);
        let over = "é".repeat(101);
        let p = plan_of(
            &findings(&[finding(&at_most, ""), finding(&over, "")]),
            &world(),
        )
        .unwrap();
        assert_eq!((p.found, p.invalid, p.file.len()), (2, 1, 1));
        let body = format!(",\"body\":\"{}\\n## Done when\\n- a\"", "😀".repeat(5900));
        assert_eq!(
            plan_of(&findings(&[finding("One", &body)]), &w)
                .unwrap()
                .invalid,
            0
        );
    }

    #[test]
    fn findings_of_the_wrong_shape_are_counted_and_dropped() {
        let w = world();
        let list = [
            finding("Good", ""),
            finding("", ""),
            finding("Bad\\tTab", ""),
            finding("Cat", r#","category":"product""#),
            finding("NoCycle", r#","cycles":[]"#),
            finding("Unknown", r#","cycles":["9-000000-000000"]"#),
            finding("Dup", r#","duplicate_of":999"#),
            finding("StringDup", r#","duplicate_of":"7""#),
            finding("Ev", r#","evidence":7"#),
            finding("Half", r#","category":"boundary","factory_part":"x""#),
            r#""a string""#.to_string(),
            "7".to_string(),
            "null".to_string(),
        ];
        let p = plan_of(&findings(&list), &w).unwrap();
        assert_eq!(p.found, 13);
        assert_eq!(p.invalid, 12);
        assert_eq!(p.file.len(), 1);
        assert_eq!(p.file[0].title, "Good");
    }

    #[test]
    fn empty_evidence_is_evidence() {
        let w = world();
        let p = plan_of(&findings(&[finding("One", r#","evidence":"""#)]), &w).unwrap();
        assert_eq!(p.invalid, 0);
    }

    #[test]
    fn a_duplicate_may_be_written_5_0() {
        let mut w = world();
        w.stack = vec![issue(5, "Other", "OPEN", &[], &[])];
        let p = plan_of(&findings(&[finding("One", r#","duplicate_of":5.0"#)]), &w).unwrap();
        assert_eq!((p.invalid, p.duplicates), (0, 1));
        assert_eq!(p.of, "#5.0");
    }

    #[test]
    fn the_names_of_duplicates_sort_as_text() {
        let mut w = world();
        w.stack = vec![
            issue(12, "Twelve", "OPEN", &[], &[]),
            issue(4, "Four", "OPEN", &[], &[]),
        ];
        let list = [
            finding("A", r#","duplicate_of":4"#),
            finding("B", r#","duplicate_of":12"#),
            finding("C", r#","duplicate_of":4"#),
        ];
        assert_eq!(plan_of(&findings(&list), &w).unwrap().of, "#12,#4");
    }

    #[test]
    fn a_title_open_or_given_before_is_a_duplicate_and_a_closed_one_is_not() {
        let mut w = world();
        w.stack = vec![
            issue(4, "the gate, runs twice!", "OPEN", &[], &[]),
            issue(5, "Closed one", "CLOSED", &[], &[]),
        ];
        let list = [
            finding("The gate runs twice", ""),
            finding("Closed one", ""),
            finding("closed ONE.", ""),
        ];
        let p = plan_of(&findings(&list), &w).unwrap();
        assert_eq!(p.duplicates, 2);
        assert_eq!(p.file.len(), 1);
        assert_eq!(p.file[0].title, "Closed one");
    }

    #[test]
    fn a_skip_counts_what_it_drops() {
        let mut w = world();
        w.skip = "1".to_string();
        w.rukas = "a/f".to_string();
        let list = [
            finding("One", r#","category":"pi-rukas""#),
            finding("Two", ""),
        ];
        let p = plan_of(&findings(&list), &w).unwrap();
        assert_eq!((p.skipped, p.file.len()), (1, 1));
    }

    #[test]
    fn three_a_run_and_each_repository_has_its_room() {
        let w = world();
        let list = [
            finding("One", ""),
            finding("Two", r#","category":"pi-rukas""#),
            finding("Three", ""),
            finding("Four", ""),
        ];
        let p = plan_of(&findings(&list), &w).unwrap();
        assert_eq!((p.file.len(), p.over), (3, 1));
        let mut w = world();
        w.rooms.insert(repo_of("a/f"), 1);
        w.rooms.insert(repo_of("a/r"), 0);
        let list = [
            finding("One", r#","category":"pi-rukas""#),
            finding("Two", ""),
            finding("Three", ""),
        ];
        let p = plan_of(&findings(&list), &w).unwrap();
        assert_eq!(p.file.len(), 1);
        assert_eq!(p.file[0].title, "Two");
        assert_eq!(p.over, 2);
    }

    #[test]
    fn a_boundary_finding_takes_room_on_both_sides_and_counts_as_two() {
        let halves = r#","category":"boundary","factory_part":"f","rukas_part":"r""#;
        let w = world();
        let list = [
            finding("One", ""),
            finding("Two", halves),
            finding("Three", ""),
        ];
        let p = plan_of(&findings(&list), &w).unwrap();
        assert_eq!((p.file.len(), p.over), (2, 1));
        assert_eq!(p.file[1].title, "Two");
        let mut w = world();
        w.rooms.insert(repo_of("a/r"), 0);
        let p = plan_of(&findings(&[finding("Two", halves)]), &w).unwrap();
        assert_eq!((p.file.len(), p.over), (0, 1));
        // With no repository for pi-rukas it is one issue.
        let mut w = world();
        w.rukas = "a/f".to_string();
        w.rooms = BTreeMap::from([(repo_of("a/f"), 1)]);
        let p = plan_of(&findings(&[finding("Two", halves)]), &w).unwrap();
        assert_eq!(p.file.len(), 1);
    }

    #[test]
    fn a_finding_speaks_of_a_repository_it_names_or_a_cycle_cited_there() {
        let w = world();
        let named = r#","evidence":"a/r: the driver and a/f: the gate""#;
        let p = plan_of(&findings(&[finding("One", named)]), &w).unwrap();
        assert_eq!(p.file[0].others, ["a/f", "a/r"]);
        assert_eq!(p.file[0].others_than("a/f"), ["a/r"]);
        let no_colon = r#","evidence":"a/r the driver""#;
        let p = plan_of(&findings(&[finding("One", no_colon)]), &w).unwrap();
        assert!(p.file[0].others.is_empty());
        let mut w = world();
        let mut cited = issue(4, "Pause", "OPEN", &[], &[REC]);
        if let J::Obj(fields) = &mut cited {
            fields[0].1 = J::str("a/r");
        }
        w.stack = vec![cited];
        let p = plan_of(&findings(&[finding("One", "")]), &w).unwrap();
        assert_eq!(p.file[0].others, ["a/r"]);
    }

    #[test]
    fn the_model_cannot_write_the_list_of_repositories() {
        let w = world();
        let p = plan_of(&findings(&[finding("One", r#","others":["a/r"]"#)]), &w).unwrap();
        assert!(p.file[0].others.is_empty());
    }

    #[test]
    fn a_stray_part_joins_as_jq_joined_it_and_a_list_there_ends_the_plan() {
        let w = world();
        let p = plan_of(&findings(&[finding("One", r#","factory_part":7"#)]), &w).unwrap();
        assert_eq!(p.file.len(), 1);
        assert!(plan_of(&findings(&[finding("One", r#","rukas_part":[1]"#)]), &w).is_none());
    }

    #[test]
    fn a_recheck_needs_an_issue_this_loop_filed_on_an_earlier_cycle() {
        let mut w = world();
        let label = "tehas:improvement";
        w.stack = vec![
            issue(12, "Filed", "OPEN", &[label], &["7-261009-100000"]),
            issue(13, "Other", "OPEN", &[], &["7-261009-100000"]),
            issue(14, "Same", "OPEN", &[label], &[REC]),
            issue(15, "Far", "OPEN", &[label], &["9-261009-100000"]),
        ];
        let r = |n: &str, more: &str| {
            format!(
                r#"{{"repo":"a/f","number":{n},"cycle":"{REC}","seen":false,"note":"n"{more}}}"#
            )
        };
        let doc =
            |list: &[String]| format!(r#"{{"findings":[],"followups":[{}]}}"#, list.join(","));
        let p = plan_of(&doc(&[r("12", "")]), &w).unwrap();
        assert_eq!(p.recheck.len(), 1);
        assert_eq!(p.recheck[0].number, "12");
        assert!(!p.recheck[0].seen);
        let p = plan_of(&doc(&[r("12.0", r#","seen":true"#)]), &w).unwrap();
        assert_eq!(p.recheck[0].number, "12.0");
        assert!(p.recheck[0].seen);
        for bad in [
            r("13", ""),
            r("14", ""),
            r("15", ""),
            r("99", ""),
            r("\"12\"", ""),
            r("12", r#","seen":"no""#),
            r("12", r#","note":"""#),
            r("12", r#","note":"a\u0007b""#),
            r("12", r#","repo":"a/other""#),
            r("12", r#","cycle":"7-261011-000000""#),
            r#""a string""#.to_string(),
        ] {
            assert!(
                plan_of(&doc(std::slice::from_ref(&bad)), &w)
                    .unwrap()
                    .recheck
                    .is_empty(),
                "{bad}"
            );
        }
        let many = "x".repeat(1501);
        assert!(
            plan_of(&doc(&[r("12", &format!(r#","note":"{many}""#))]), &w)
                .unwrap()
                .recheck
                .is_empty()
        );
    }

    #[test]
    fn followups_that_are_not_a_list_are_no_rechecks() {
        let w = world();
        let p = plan_of(r#"{"findings":[],"followups":"all"}"#, &w).unwrap();
        assert!(p.recheck.is_empty());
    }

    #[test]
    fn rechecks_are_sorted_once_each_and_ten_at_most() {
        let mut w = world();
        let label = "tehas:improvement";
        w.stack = (1..=12)
            .map(|n| issue(n, "Filed", "CLOSED", &[label], &["7-261009-100000"]))
            .collect();
        let list: Vec<String> = (1..=12)
            .rev()
            .chain([3])
            .map(|n| {
                format!(
                    r#"{{"repo":"a/f","number":{n},"cycle":"{REC}","seen":true,"note":"n{n}"}}"#
                )
            })
            .collect();
        let doc = format!(r#"{{"findings":[],"followups":[{}]}}"#, list.join(","));
        let p = plan_of(&doc, &w).unwrap();
        let numbers: Vec<&str> = p.recheck.iter().map(|r| r.number.as_str()).collect();
        assert_eq!(numbers, ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"]);
    }

    #[test]
    fn a_stack_entry_keeps_its_fields_and_cuts_its_body() {
        let long = "é".repeat(4100);
        let list = format!(
            r#"[{{"number":4,"title":"T","state":"OPEN","labels":[{{"name":"a"}}],"body":"{long}"}},
                {{"number":5,"title":"U","state":"OPEN","labels":[],"body":null}}]"#
        );
        let got = issues_of("a/f", &[&list, "[]"]).unwrap();
        assert_eq!(got.len(), 2);
        assert_eq!(got[0].get("body").as_str().unwrap().chars().count(), 4000);
        assert_eq!(
            got[1].dump(),
            r#"{"repo":"a/f","number":5,"title":"U","state":"OPEN","labels":[],"cycles":[],"body":""}"#
        );
        for bad in [
            "{}",
            "not json",
            r#"[{"number":1}]"#,
            r#"[{"labels":["a"]}]"#,
            r#"[{"labels":[],"body":7}]"#,
        ] {
            assert!(issues_of("a/f", &[bad]).is_none(), "{bad}");
        }
    }

    #[test]
    fn an_issue_rests_on_the_cycles_of_its_cycles_lines() {
        let body = "x **Cycles:** 7-261009-100000, nope, 8-261009-100000\r\n\n**Cycles:** 9-261009-100000\n**Cycles:** ";
        let cycles = cycles_of(body);
        let names: Vec<&str> = cycles.iter().filter_map(J::as_str).collect();
        assert_eq!(names, ["7-261009-100000", "9-261009-100000"]);
    }

    #[test]
    fn room_is_what_the_limit_leaves_and_never_below_nothing() {
        let label = "tehas:improvement";
        let stack: Vec<J> = (1..=3)
            .map(|n| issue(n, "T", "OPEN", &[label], &[]))
            .chain([
                issue(4, "T", "CLOSED", &[label], &[]),
                issue(5, "T", "OPEN", &[], &[]),
            ])
            .collect();
        let repos = [repo_of("a/f"), repo_of("a/r")];
        let got = rooms(&stack, &repos, label, 2);
        assert_eq!(got["a/f"], 0);
        assert_eq!(got["a/r"], 2);
    }

    #[test]
    fn what_a_stopped_analyst_named_is_read_from_any_shape() {
        let doc = J::parse(&format!(
            r#"{{"findings":[{{"title":1,"cycles":["{REC}",7]}},"x",{{"cycles":"no"}}],
              "followups":[{{"cycle":"1-000000-000000"}},"x",{{"cycle":7}}]}}"#
        ))
        .unwrap();
        assert_eq!(named(&doc), [REC, "1-000000-000000"]);
        assert!(named(&J::parse(r#"{"findings":"no"}"#).unwrap()).is_empty());
    }
}
