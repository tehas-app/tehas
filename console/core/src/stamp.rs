//! The stamp of who built a cycle's work: in-house, or a contractor and the
//! job the factory gave it. The factory writes it, never the contractor: the
//! line on the pull request and on the issue's comments, the `Built-by:`
//! trailer of a contractor's commits, and the contractor on the run in the
//! analytics. Only an instance with the outsource feature on writes any of it.

use serde_json::Value;

/// The key of the commit trailer.
pub const TRAILER: &str = "Built-by";

/// The line of a cycle run on this host.
pub const IN_HOUSE: &str = "Built by: in-house";

/// How the line of a contractor's cycle begins.
const BY_CONTRACTOR: &str = "Built by contractor: ";

/// The longest name a contractor may have, as the store holds it.
const NAME_MAX: usize = 64;

/// Who built a cycle's work.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Built {
    InHouse,
    Contractor { name: String, job: i64 },
}

impl Built {
    /// The store's answer to `job-stamp`: the job's contractor, or in-house
    /// when its job is null. An answer whose name or number is not of
    /// their shape is no job.
    pub fn of_answer(answer: &Value) -> Built {
        if answer.get("verb").and_then(Value::as_str) != Some("job-stamp") {
            return Built::InHouse;
        }
        let name = answer
            .get("contractor")
            .and_then(Value::as_str)
            .and_then(name);
        let job = answer.get("job").and_then(Value::as_i64).filter(|j| *j > 0);
        match (name, job) {
            (Some(name), Some(job)) => Built::Contractor {
                name: name.to_string(),
                job,
            },
            _ => Built::InHouse,
        }
    }

    /// The line the pull request and the comments carry.
    pub fn line(&self) -> String {
        match self {
            Built::InHouse => IN_HOUSE.to_string(),
            Built::Contractor { name, job } => format!("{BY_CONTRACTOR}{name} (job {job})"),
        }
    }

    /// The contractor's name, the value of its commits' trailer.
    pub fn contractor(&self) -> Option<&str> {
        match self {
            Built::InHouse => None,
            Built::Contractor { name, .. } => Some(name),
        }
    }
}

/// A contractor's name as the store takes it: a host's name, up to 64
/// characters of letters, digits, `.`, `-` and `_`, starting with a letter or
/// a digit. Nothing wider reaches a comment or a commit.
pub fn name(s: &str) -> Option<&str> {
    let b = s.as_bytes();
    let ok = !b.is_empty()
        && b.len() <= NAME_MAX
        && b[0].is_ascii_alphanumeric()
        && b.iter()
            .all(|&c| c.is_ascii_alphanumeric() || b"._-".contains(&c));
    ok.then_some(s)
}

fn is_stamp_line(line: &str) -> bool {
    let l = line.trim();
    l == IN_HOUSE || l.starts_with(BY_CONTRACTOR)
}

/// `body` with the stamp as its last line: any stamp line it held is taken
/// out first, so the line is there once and says what is true now.
pub fn with_line(body: &str, built: &Built) -> String {
    let kept: Vec<&str> = body.lines().filter(|l| !is_stamp_line(l)).collect();
    let text = kept.join("\n");
    let text = text.trim_end();
    if text.is_empty() {
        built.line()
    } else {
        format!("{text}\n\n{}", built.line())
    }
}

/// Whether a line is a trailer, `Key: value` with a key of letters, digits
/// and `-`.
fn is_trailer(line: &str) -> bool {
    match line.split_once(':') {
        Some((key, _)) => {
            !key.is_empty() && key.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-')
        }
        None => false,
    }
}

fn is_built_by(line: &str) -> bool {
    line.split_once(':')
        .is_some_and(|(key, _)| key.eq_ignore_ascii_case(TRAILER))
}

/// A commit message split into its text and its last paragraph when that
/// paragraph is all trailers, as git reads them.
fn split(msg: &str) -> (Vec<&str>, Vec<&str>) {
    let lines: Vec<&str> = msg.trim_end().lines().map(str::trim_end).collect();
    let start = lines
        .iter()
        .rposition(|l| l.trim().is_empty())
        .map_or(0, |i| i + 1);
    let last = &lines[start..];
    // A message of one paragraph is its subject, never a trailer block.
    if start > 0 && !last.is_empty() && last.iter().all(|l| is_trailer(l)) {
        (lines[..start].to_vec(), last.to_vec())
    } else {
        (lines, Vec::new())
    }
}

/// Whether the message carries exactly one `Built-by:` trailer and it names
/// `name`.
pub fn trailer_ok(msg: &str, name: &str) -> bool {
    let (_, trailers) = split(msg);
    let by: Vec<&str> = trailers
        .iter()
        .filter(|l| is_built_by(l))
        .copied()
        .collect();
    by.len() == 1 && by[0].split_once(':').map(|(_, v)| v.trim()) == Some(name)
}

/// The message with one `Built-by: <name>` trailer: any other `Built-by:`
/// trailer is taken out, and the rest of the message is kept as it was.
pub fn with_trailer(msg: &str, name: &str) -> String {
    let (mut text, trailers) = split(msg);
    let mut kept: Vec<String> = trailers
        .iter()
        .filter(|l| !is_built_by(l))
        .map(|l| l.to_string())
        .collect();
    kept.push(format!("{TRAILER}: {name}"));
    while text.last().is_some_and(|l| l.trim().is_empty()) {
        text.pop();
    }
    let text = text.join("\n");
    if text.is_empty() {
        format!("{}\n", kept.join("\n"))
    } else {
        format!("{text}\n\n{}\n", kept.join("\n"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn alpha() -> Built {
        Built::Contractor {
            name: "box-1".into(),
            job: 12,
        }
    }

    #[test]
    fn the_line_names_the_contractor_and_its_job_or_says_in_house() {
        assert_eq!(alpha().line(), "Built by contractor: box-1 (job 12)");
        assert_eq!(Built::InHouse.line(), "Built by: in-house");
    }

    #[test]
    fn the_answer_of_the_store_names_a_job_or_it_is_in_house() {
        let job =
            json!({"api": 1, "verb": "job-stamp", "ok": true, "job": 12, "contractor": "box-1"});
        assert_eq!(Built::of_answer(&job), alpha());
        let none =
            json!({"api": 1, "verb": "job-stamp", "ok": true, "job": null, "contractor": null});
        assert_eq!(Built::of_answer(&none), Built::InHouse);
        for bad in [
            json!({"verb": "job-stamp", "job": 12, "contractor": "box 1"}),
            json!({"verb": "job-stamp", "job": 12, "contractor": "*x*"}),
            json!({"verb": "job-stamp", "job": 0, "contractor": "box-1"}),
            json!({"verb": "job-stamp", "contractor": "box-1"}),
            json!({"verb": "job", "job": 12, "contractor": "box-1"}),
            json!({"verb": "refused"}),
            json!(null),
        ] {
            assert_eq!(Built::of_answer(&bad), Built::InHouse, "{bad}");
        }
    }

    #[test]
    fn the_line_ends_the_body_once_and_replaces_an_older_one() {
        let body = "Closes #7.\n\nWhat changed.";
        let once = with_line(body, &alpha());
        assert_eq!(
            once,
            "Closes #7.\n\nWhat changed.\n\nBuilt by contractor: box-1 (job 12)"
        );
        assert_eq!(with_line(&once, &alpha()), once);
        assert_eq!(
            with_line(&once, &Built::InHouse),
            "Closes #7.\n\nWhat changed.\n\nBuilt by: in-house"
        );
        assert_eq!(with_line("", &Built::InHouse), "Built by: in-house");
    }

    #[test]
    fn a_trailer_is_added_or_rewritten_and_the_other_trailers_kept() {
        let plain = "Fix the parser\n\nIt read past the end.\n";
        assert!(!trailer_ok(plain, "box-1"));
        let added = with_trailer(plain, "box-1");
        assert_eq!(
            added,
            "Fix the parser\n\nIt read past the end.\n\nBuilt-by: box-1\n"
        );
        assert!(trailer_ok(&added, "box-1"));
        assert_eq!(with_trailer(&added, "box-1"), added);

        let wrong = "Fix the parser\n\nSigned-off-by: A <a@b>\nBuilt-by: other\n";
        assert!(!trailer_ok(wrong, "box-1"));
        assert_eq!(
            with_trailer(wrong, "box-1"),
            "Fix the parser\n\nSigned-off-by: A <a@b>\nBuilt-by: box-1\n"
        );

        let twice = "Fix\n\nBuilt-by: box-1\nbuilt-by: box-1\n";
        assert!(!trailer_ok(twice, "box-1"));
        assert_eq!(with_trailer(twice, "box-1"), "Fix\n\nBuilt-by: box-1\n");

        // A subject alone is no trailer block, whatever it looks like.
        assert_eq!(
            with_trailer("Built-by: other", "box-1"),
            "Built-by: other\n\nBuilt-by: box-1\n"
        );
    }

    #[test]
    fn a_name_is_a_hosts_name_and_nothing_wider() {
        assert_eq!(name("box-1.lan_2"), Some("box-1.lan_2"));
        for bad in ["", "-box", "box 1", "box`1", "b@x", &"b".repeat(65)] {
            assert_eq!(name(bad), None, "{bad}");
        }
    }
}
