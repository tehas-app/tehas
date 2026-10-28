//! The review issue's title and body, from the document of one review run.
//!
//! A review of the default branch answers with one JSON document holding the
//! findings of the six lenses, one object each, and the command that ran it
//! keeps that document as the log of the run. The findings are filed as one
//! issue, so a person reads them before any is acted on. What that issue says
//! is this rule: which findings are listed, in what order, and how much of
//! each is shown. It is a function of the document alone, so one log always
//! renders one issue, and the rule is tested without a review having run.
//!
//! Three severities are listed one to a line and the rest are counted,
//! because an issue of a hundred lines is not read. Of a listed finding the
//! lens, path, line and title are shown and nothing else: what a finding says
//! at length belongs in the log the run left, not in the issue. The words of
//! a finding come from the model, so each is put on one line and cut short,
//! and the count in the title is the count in the body: a finding that
//! cannot be shown is never left out of the total.

use serde_json::Value;

/// The most characters of one piece of model text. A finding whose title runs
/// on tells nothing more by running further.
pub const MAX_CHARS: usize = 200;

/// The severities listed one to a line, in the order their sections come. Any
/// other severity is counted with the findings that are not listed.
const SECTIONS: [(&str, &str); 3] = [
    ("CRITICAL", "### Critical"),
    ("HIGH", "### High"),
    ("MEDIUM", "### Medium"),
];

/// The issue the renderer returns: its title and its body, neither with a
/// trailing newline.
#[derive(Debug, PartialEq, Eq)]
pub struct Issue {
    pub title: String,
    pub body: String,
}

/// A finding of the document: the four fields the issue shows, and the words
/// of its `line` when the document gave one at all.
struct Found<'a> {
    severity: &'a str,
    lens: &'a str,
    path: &'a str,
    line: Option<String>,
    shown: bool,
    title: &'a str,
}

/// The elements of `findings`, in the order the document gave them.
///
/// Each holds what the document said, with no promise about its kind. The
/// rule of what can be shown is `is_listed`, which reads these fields: a
/// finding the issue cannot show is counted with the ones it does not list,
/// so the title's count and the body's lines come from one pass over the
/// array. The document's `line` is kept as its words whatever it holds, and
/// is shown only when those words are a number: the document of a run writes
/// it as one.
fn findings(document: &Value) -> Vec<Found<'_>> {
    match document.get("findings").and_then(Value::as_array) {
        Some(items) => items
            .iter()
            .map(|f| {
                let text = |key: &str| f.get(key).and_then(Value::as_str).unwrap_or("");
                Found {
                    severity: text("severity"),
                    lens: text("lens"),
                    path: text("path"),
                    line: f.get("line").map(|v| v.to_string()),
                    // The four fields the issue shows must all be text: one
                    // of them missing, or held as another kind of value, and
                    // the finding cannot be shown.
                    shown: ["severity", "lens", "path", "title"]
                        .iter()
                        .all(|key| f.get(*key).and_then(Value::as_str).is_some()),
                    title: text("title"),
                }
            })
            .collect(),
        None => Vec::new(),
    }
}

/// The review issue for one run's document, its branch, and the short commit
/// the range starts at.
///
/// `findings` is all of the document that is read; the rest of it belongs to
/// the run. A finding is listed when its severity is one of the three words of
/// `SECTIONS`, exactly and in upper case, and its lens, path and title are all
/// text. Every other element, and a document whose `findings` is missing or is
/// not an array, is counted with the findings not listed, so the number in the
/// title and the lines of the body agree.
pub fn review_issue(document: &Value, branch: &str, commit: &str) -> Issue {
    let all = findings(document);
    // The findings the issue shows a line for, and the count of the rest of
    // the array. The count is taken from what the document holds rather than
    // from what can be shown, so a finding of a shape the issue cannot render
    // is still in the number the title gives and the two agree.
    let rows: Vec<&Found> = all.iter().filter(|f| is_listed(f)).collect();
    let low = all.len() - rows.len();

    let mut body = String::new();
    for (severity, heading) in SECTIONS {
        let some: Vec<&Found> = rows
            .iter()
            .copied()
            .filter(|f| f.severity == severity)
            .collect();
        if some.is_empty() {
            continue;
        }
        // One blank line between the sections, and none before the first.
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(heading);
        body.push('\n');
        for f in some {
            body.push_str(&finding_line(f.lens, f.path, f.line.as_deref(), f.title));
            body.push('\n');
        }
    }

    if low > 0 {
        if !body.is_empty() {
            body.push('\n');
        }
        body.push_str(&format!("Low: {low} more findings not listed.\n"));
    }

    Issue {
        title: format!(
            "Six-lens review: {} findings on {branch} since {commit}",
            all.len()
        ),
        body: body.trim_end_matches('\n').to_string(),
    }
}

/// The findings the issue shows a line for: `shown`, and a severity that
/// names one of the sections.
fn is_listed(f: &Found) -> bool {
    f.shown && SECTIONS.iter().any(|(s, _)| *s == f.severity)
}

/// One finding's line: the lens, the path with its line when the document
/// gives one that is a number, and the title. A `line` of any other value
/// leaves the path alone, because it locates nothing.
fn finding_line(lens: &str, path: &str, line: Option<&str>, title: &str) -> String {
    let at = match line {
        Some(n) if n.parse::<u64>().is_ok() => format!("{path}:{n}"),
        _ => path.to_string(),
    };
    format!("- [{}] {}: {}", short(lens), short(&at), short(title))
}

/// Model text on one line: carriage returns and newlines become spaces, and no
/// more than `MAX_CHARS` characters are kept.
fn short(text: &str) -> String {
    text.chars()
        .map(|c| if c == '\n' || c == '\r' { ' ' } else { c })
        .take(MAX_CHARS)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    const BRANCH: &str = "main";
    const HEAD: &str = "cb6912a";

    /// A finding as the run's document holds it. `line` is what the document
    /// gives for it, which need not be a number.
    fn finding(severity: &str, lens: &str, path: &str, line: Value, title: &str) -> Value {
        json!({"severity": severity, "lens": lens, "path": path,
               "line": line, "title": title,
               "description": "what the finding says at length",
               "suggestion": "what to do about it"})
    }

    /// A finding of the document a real review answers with: its line is a
    /// number.
    fn located(severity: &str, lens: &str, path: &str, line: u64, title: &str) -> Value {
        finding(severity, lens, path, json!(line), title)
    }

    /// A document of the run's shape around these findings.
    fn document(findings: Vec<Value>) -> Value {
        json!({"exitCode": 0, "verdict": "ISSUES_FOUND", "noReview": false,
               "findings": findings})
    }

    #[test]
    fn the_title_names_the_count_the_branch_and_the_commit() {
        let issue = review_issue(&document(Vec::new()), BRANCH, HEAD);
        assert_eq!(
            issue.title,
            "Six-lens review: 0 findings on main since cb6912a"
        );
    }

    #[test]
    fn no_findings_is_an_empty_body() {
        for doc in [
            document(Vec::new()),
            json!({"exitCode": 0}),
            json!({"findings": null}),
            json!({"findings": "none"}),
            json!({"findings": {}}),
            json!([]),
            json!("not an object"),
        ] {
            let issue = review_issue(&doc, BRANCH, HEAD);
            assert_eq!(issue.body, "", "{doc}");
            assert!(issue.title.contains("0 findings"), "{doc}");
        }
    }

    #[test]
    fn three_findings_are_grouped_by_severity_and_low_is_counted() {
        let doc = document(vec![
            located(
                "MEDIUM",
                "simplicity",
                "console/store/src/db.rs",
                88,
                "a copy",
            ),
            located(
                "CRITICAL",
                "security",
                "console/src/review.rs",
                12,
                "a token",
            ),
            finding(
                "LOW",
                "performance",
                "console/core/src/time.rs",
                json!(7),
                "a loop",
            ),
            located(
                "HIGH",
                "architecture",
                "console/store/src/verbs.rs",
                3,
                "a cycle",
            ),
            finding(
                "LOW",
                "security",
                "console/src/render.rs",
                json!(4),
                "another",
            ),
        ]);
        let issue = review_issue(&doc, BRANCH, HEAD);
        assert_eq!(
            issue.body,
            concat!(
                "### Critical\n",
                "- [security] console/src/review.rs:12: a token\n\n",
                "### High\n",
                "- [architecture] console/store/src/verbs.rs:3: a cycle\n\n",
                "### Medium\n",
                "- [simplicity] console/store/src/db.rs:88: a copy\n\n",
                "Low: 2 more findings not listed."
            )
        );
        assert_eq!(
            issue.title,
            "Six-lens review: 5 findings on main since cb6912a"
        );
    }

    #[test]
    fn the_whole_body_of_one_finding_of_each_severity_is_the_fixture() {
        let doc = document(vec![
            located("CRITICAL", "security", "lib/config.sh", 4, "a secret"),
            located(
                "HIGH",
                "architecture",
                "console/store/src/db.rs",
                17,
                "a lock",
            ),
            located(
                "MEDIUM",
                "simplicity",
                "console/src/render.rs",
                2,
                "a branch",
            ),
            finding(
                "LOW",
                "performance",
                "console/core/src/slow.rs",
                json!(9),
                "a read",
            ),
            finding(
                "LOW",
                "type-safety",
                "console/analytics/src/main.rs",
                json!(5),
                "a cast",
            ),
            finding(
                "LOW",
                "error-handling",
                "host/docs/api.md",
                json!(1),
                "a swallow",
            ),
        ]);
        let issue = review_issue(&doc, BRANCH, HEAD);
        assert_eq!(
            issue.body,
            concat!(
                "### Critical\n",
                "- [security] lib/config.sh:4: a secret\n\n",
                "### High\n",
                "- [architecture] console/store/src/db.rs:17: a lock\n\n",
                "### Medium\n",
                "- [simplicity] console/src/render.rs:2: a branch\n\n",
                "Low: 3 more findings not listed."
            )
        );
        assert_eq!(
            issue.title,
            "Six-lens review: 6 findings on main since cb6912a"
        );
        assert!(!issue.body.ends_with('\n'), "{:?}", issue.body);
        // What a finding says at length stays in the log of the run.
        assert!(!issue.body.contains("at length"));
        assert!(!issue.body.contains("what to do"));
    }

    #[test]
    fn a_section_is_there_only_when_it_has_findings() {
        let doc = document(vec![
            located("HIGH", "architecture", "a.rs", 1, "a"),
            located("MEDIUM", "simplicity", "b.rs", 2, "b"),
        ]);
        let issue = review_issue(&doc, BRANCH, HEAD);
        assert_eq!(
            issue.body,
            concat!(
                "### High\n",
                "- [architecture] a.rs:1: a\n\n",
                "### Medium\n",
                "- [simplicity] b.rs:2: b"
            )
        );
        assert!(!issue.body.contains("Critical"), "{}", issue.body);
    }

    #[test]
    fn the_lines_of_a_section_stay_in_the_order_of_the_document() {
        let doc = document(vec![
            located("HIGH", "security", "b.rs", 2, "second"),
            located("HIGH", "simplicity", "a.rs", 1, "first"),
        ]);
        assert_eq!(
            review_issue(&doc, BRANCH, HEAD).body,
            "### High\n- [security] b.rs:2: second\n- [simplicity] a.rs:1: first"
        );
    }

    #[test]
    fn a_finding_whose_line_is_not_a_number_shows_the_path_alone() {
        // A missing, null, blank or non-numeric `line` is no location, so the
        // path stands alone and nothing is guessed at.
        for line in [
            Value::Null,
            json!(""),
            json!("twelve"),
            json!({"n": 3}),
            json!([]),
        ] {
            let doc = document(vec![finding("HIGH", "security", "a.rs", line.clone(), "a")]);
            let issue = review_issue(&doc, BRANCH, HEAD);
            assert_eq!(issue.body, "### High\n- [security] a.rs: a", "{line}");
        }
    }

    #[test]
    fn the_line_of_a_finding_shows_the_number_the_document_gives() {
        let cases: [(Value, &str); 4] = [
            (json!(12), "- [security] a.rs:12: a"),
            (json!(0), "- [security] a.rs:0: a"),
            (json!(7), "- [security] a.rs:7: a"),
            (json!(44), "- [security] a.rs:44: a"),
        ];
        for (line, want) in cases {
            let doc = document(vec![finding("HIGH", "security", "a.rs", line.clone(), "a")]);
            assert_eq!(
                review_issue(&doc, BRANCH, HEAD).body,
                format!("### High\n{want}"),
                "{line}"
            );
        }
    }

    #[test]
    fn a_title_of_over_200_characters_comes_back_exactly_200() {
        let long = "x".repeat(MAX_CHARS + 40);
        let doc = document(vec![located("HIGH", "security", "a.rs", 1, &long)]);
        let shown = review_issue(&doc, BRANCH, HEAD).body;
        let title = shown.lines().last().unwrap().rsplit(": ").next().unwrap();
        assert_eq!(title.chars().count(), MAX_CHARS, "{title}");
    }

    #[test]
    fn the_cut_of_a_title_counts_characters_and_not_bytes() {
        // 200 multi-byte characters are kept whole, so the line runs past 200
        // bytes and no character is written in halves.
        let long = "é".repeat(MAX_CHARS + 40);
        let doc = document(vec![located("HIGH", "security", "a.rs", 1, &long)]);
        let shown = review_issue(&doc, BRANCH, HEAD).body;
        let title = shown.lines().last().unwrap().rsplit(": ").next().unwrap();
        assert_eq!(title.chars().count(), MAX_CHARS);
        assert_eq!(title, "é".repeat(MAX_CHARS));
    }

    #[test]
    fn model_text_with_newlines_is_put_on_one_line() {
        let doc = document(vec![finding(
            "HIGH",
            "sec\nurity",
            "a\nrs/b.rs",
            json!(1),
            "a title\nwith\r\nbreaks",
        )]);
        let issue = review_issue(&doc, BRANCH, HEAD);
        assert_eq!(
            issue.body,
            "### High\n- [sec urity] a rs/b.rs:1: a title with  breaks"
        );
        assert_eq!(issue.body.lines().count(), 2);
    }

    #[test]
    fn a_finding_of_another_shape_is_counted_and_not_listed() {
        let odd = [
            json!(42),
            json!("a finding"),
            json!(null),
            json!([]),
            json!({"severity": "HIGH", "lens": "security", "path": "a.rs"}),
            json!({"severity": "HIGH", "lens": 3, "path": "a.rs", "title": "t"}),
            json!({"severity": "HIGH", "path": "a.rs", "title": "t"}),
            json!({"severity": "HIGH", "lens": "security", "title": "t"}),
            json!({"lens": "security", "path": "a.rs", "title": "t"}),
        ];
        let mut findings = vec![located("HIGH", "security", "kept.rs", 1, "kept")];
        findings.extend(odd.iter().cloned());
        let issue = review_issue(&document(findings), BRANCH, HEAD);
        assert_eq!(
            issue.body,
            "### High\n- [security] kept.rs:1: kept\n\nLow: 9 more findings not listed."
        );
        assert!(issue.title.contains("10 findings"), "{}", issue.title);
    }

    #[test]
    fn a_severity_outside_the_three_is_counted_with_low() {
        for severity in ["LOW", "low", "High", "critical", "", "MEDIUM ", "UNKNOWN"] {
            let doc = document(vec![located(severity, "security", "a.rs", 1, "a")]);
            let issue = review_issue(&doc, BRANCH, HEAD);
            assert_eq!(
                issue.body, "Low: 1 more findings not listed.",
                "{severity:?}"
            );
            assert!(issue.title.contains("1 findings"), "{severity:?}");
        }
    }

    #[test]
    fn findings_of_the_count_only_are_one_line() {
        let doc = document(vec![
            located("LOW", "security", "a.rs", 1, "a"),
            located("LOW", "simplicity", "b.rs", 2, "b"),
        ]);
        assert_eq!(
            review_issue(&doc, BRANCH, HEAD).body,
            "Low: 2 more findings not listed."
        );
    }

    #[test]
    fn the_document_is_not_changed_by_rendering() {
        let doc = document(vec![located("HIGH", "security", "a.rs", 1, "a")]);
        let before = doc.to_string();
        review_issue(&doc, BRANCH, HEAD);
        assert_eq!(doc.to_string(), before);
    }

    #[test]
    fn a_token_shaped_title_reaches_the_body_as_the_document_writes_it() {
        // The renderer is pure: it puts the words on one line and cuts them,
        // and masks nothing. Hiding a host's own names and a token a finding
        // quoted is the work of the half that files the issue, which takes
        // these two values through the masked path the console already has.
        // What the renderer guarantees is that such a title reaches the body
        // as one line and no longer than any other title, so the masked text
        // of the filing half cannot run over into another line.
        // Synthetic filler, the shape the workspace uses for token fixtures
        // (console/src/export.rs, console/core/src/mask.rs): a real token
        // pattern with no token in it, so no secret scan fires on this file.
        let secret = format!("github_pat_{}", "A".repeat(36));
        let doc = document(vec![located(
            "CRITICAL",
            "security",
            "instances/example/settings.sh",
            7,
            &format!("a token is printed: {secret}"),
        )]);
        let issue = review_issue(&doc, BRANCH, HEAD);
        assert_eq!(
            issue.body,
            format!("### Critical\n- [security] instances/example/settings.sh:7: a token is printed: {secret}")
        );
        assert_eq!(issue.body.lines().count(), 2, "{}", issue.body);
        assert!(issue.title.contains("1 findings"), "{}", issue.title);
    }
}
