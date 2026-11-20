//! The code of a hold the poll logged for one issue: `state/starts.json`
//! (tehas_core::starts) names each reason by one, beside the poll's own line.
//! A line no arm knows is `other`, with its text: the poll still held it.

use tehas_core::starts::code;

/// The issue's own line of the poll, without its `wait #n: ` and the like,
/// when `line` is one.
pub fn of_issue(line: &str, n: u64) -> Option<&str> {
    ["wait", "skip", "hold", "clear"]
        .iter()
        .find_map(|w| line.strip_prefix(&format!("{w} #{n}: ")))
}

/// The code of a line `of_issue` gave. The first arm that matches wins, so
/// the narrow ones come first.
pub fn code_of(text: &str) -> &'static str {
    const ARMS: &[(&str, &str)] = &[
        ("not in the start order", code::NOT_IN_ORDER),
        ("its cycle is still running", code::RUNNING),
        ("a planning session is open", code::PLANNING),
        ("the control issue is never worked on", code::CONTROL),
        ("a closed issue is never taken", code::CLOSED),
        ("it is a pull request", code::NOT_ISSUE),
        ("a blocked-by link holds it", code::BLOCKED_LINK),
        ("tehas:blocked holds it", code::BLOCKED),
        ("is open and closes it", code::OPEN_PR),
        ("an epic is never started", code::EPIC),
        ("TEHAS_SIZE_MAX_FILES", code::TOO_LARGE),
        // Rule 4: a tick on the bot's question, and the question itself.
        (
            "a box was ticked in a question this host has no record of",
            code::QUESTION,
        ),
        ("are no answer", code::QUESTION),
        ("its question is not among", code::QUESTION),
        ("its question was written by", code::QUESTION),
        ("the question on record does not belong", code::QUESTION),
        (
            "the park of a lean start this tehas cannot run",
            code::DRIVER,
        ),
        (
            "a retry was ticked but the issue is not parked",
            code::PARKED,
        ),
        (
            "its retry after a provider outage is not started",
            code::PARKED,
        ),
        ("its last cycle parked", code::PARKED),
        ("its park on record", code::PARKED),
        ("changed since the host approved it", code::TEXT_CHANGED),
        ("its text was changed by", code::TEXT_CHANGED),
        ("who may not authorise work here", code::NOT_ALLOWED),
        ("not an allowed account", code::NOT_ALLOWED),
        ("was taken off by", code::LABEL_OFF),
        ("from the fixer", code::MILLWRIGHT),
        ("TEHAS_MILLWRIGHT_MAX_FEEDS", code::MILLWRIGHT),
        ("from the bot starts nothing", code::NOT_ALLOWED),
        ("tehas:ready was applied by", code::NOT_ALLOWED),
        ("it carries both", code::DRIVER),
        ("block for the lean driver to read", code::SPEC),
        ("the lean driver", code::DRIVER),
        ("tehas does not guess the driver", code::DRIVER),
        ("TEHAS_DRIVER=lean", code::DRIVER),
        ("attachment", code::ATTACHMENT),
        // The rule of a text that says a file goes there (rule 6).
        ("its text says", code::ATTACHMENT),
        ("read-only, and this host started it", code::READ_ONLY),
        ("the refresh lane is full", code::REFRESH_LANE),
        ("carries tehas:refreshing", code::REFRESH_LANE),
        ("GitHub has not said yet", code::UNREADABLE),
        ("Spec:", code::SPEC),
        ("could not be read", code::UNREADABLE),
    ];
    ARMS.iter()
        .find(|(words, _)| text.contains(words))
        .map_or(code::OTHER, |(_, c)| c)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_lines_of_the_rules_have_their_codes() {
        for (line, want) in [
            ("wait #8: not in the start order (--only)", code::NOT_IN_ORDER),
            ("wait #8: its cycle is still running", code::RUNNING),
            ("skip #8: a planning session is open on it; end it first", code::PLANNING),
            ("skip #8: it is closed, and a closed issue is never taken", code::CLOSED),
            ("wait #8: a blocked-by link holds it: 2 blockers are open", code::BLOCKED_LINK),
            ("wait #8: tehas:blocked holds it: #3 is open", code::BLOCKED),
            ("skip #8: pull request https://github.com/acme/widget/pull/9 is open and closes it", code::OPEN_PR),
            ("skip #8: it is an epic with 2 sub-issues open, and an epic is never started", code::EPIC),
            ("hold #8: its last cycle parked on underspecified and the body has not changed since", code::PARKED),
            ("clear #8: tehas:ready off, applied by 'mallory' who may not authorise work here", code::NOT_ALLOWED),
            ("skip #8: tehas:ready was taken off by 'bob' since it was applied", code::LABEL_OFF),
            ("wait #8: its approval could not be read", code::UNREADABLE),
            (
                "skip #8: its text holds no `spec` block for the lean driver to read: it is not there",
                code::SPEC,
            ),
            (
                "skip #8: it carries both tehas:lean and tehas:classic; tehas does not guess the driver, so nothing starts until one comes off",
                code::DRIVER,
            ),
            ("skip #8: something a later rule says", code::OTHER),
        ] {
            let text = of_issue(line, 8).expect(line);
            assert_eq!(code_of(text), want, "{line}");
        }
        assert_eq!(of_issue("wait #81: its cycle is still running", 8), None);
        assert_eq!(of_issue("would run a cycle for #8", 8), None);
    }
}
