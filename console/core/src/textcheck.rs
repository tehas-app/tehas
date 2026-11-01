//! One text check for what tehas writes to an issue.
//!
//! A planned title has been published as `EPIC: EPIC` and a body has been
//! published as the planner's brief echoed back: `Descriptor:` with a
//! `Prior context inventory` heading and nothing of the issue in it. Neither
//! says what the issue is. This module says no, in words, before the text
//! reaches GitHub: [`check_title`] for the title and [`check_body`] for the
//! body.
//!
//! The reason a check gives is what the writer puts in the comment that tells
//! a person the draft was refused, so a person reads it on the issue: the
//! wording matters, and one refusal is one line of at most
//! [`MAX_REASON_CHARS`] characters.
//!
//! The rules read the text the way a person does, in the two ways a person
//! reads. A title is read word by word: split on whitespace and compare
//! without case or the punctuation around a word, and a word left with
//! nothing is not a word, so a title of only punctuation is empty. A body is
//! read as whole strings: the brief's marks are looked for in the text as it
//! stands, because each is a phrase rather than a word. Before either rule
//! runs, the characters a reader never sees are taken out, so hiding a letter
//! inside a word cannot make a label, a repeated word or one of the brief's
//! marks unreadable, and every rule reads that one text.
//!
//! Two rules of #1015 are deliberately not here: how many words a title
//! holds, and the title repeating the body's words. The first would refuse
//! short titles that do say what the issue is. The second needs the
//! planner's inventory, which the planner holds, so the writer that has it
//! answers it. No regex, and nothing here is behind a feature.

use std::fmt;

/// The most characters of the text a refusal repeats. A refusal is one line
/// of the comment that says why a draft was refused; a title that runs on
/// tells nothing more by running further.
pub const MAX_REASON_CHARS: usize = 200;

/// The most characters of a title or a body that is read at all. Past it the
/// text says nothing the first so many characters of it do not, so a check
/// stops there instead of copying whatever a turn of the model emitted.
const MAX_TITLE_CHARS: usize = 10_000;
const MAX_BODY_CHARS: usize = 100_000;

/// The words a title may open with and still be a title: the six
/// classifications `host/docs/operating.md` says a plan's title carries
/// (`feat`, `fix`, `chore`, `docs`, `test`, `refactor`), the `research` the
/// planner adds (`console/src/plan.rs`), and `epic` for an epic, which the
/// manual defines structurally rather than as a prefix. A title of nothing
/// but one of these is a label, not a summary. Case does not matter, and the
/// colon may be there or not: `words` takes the colon off `epic:`, so each
/// word is written once here.
const LABELS: &[&str] = &[
    "epic", "feat", "fix", "chore", "docs", "test", "refactor", "research",
];

/// The two marks of the planner's brief, which together are the form the model
/// was given. `Descriptor` on its own is a field a body may well hold, and so is
/// a section that invents prior context; both together, each at the head of the
/// text it is read from, are the form echoed back, which is what #898 published
/// as its issue.
const BRIEF_FIELD: &str = "descriptor:";
const BRIEF_INVENTORY: &str = "prior context inventory";

/// The marker a brief leaves where its answer should be, on a line of its
/// own. A body of nothing but these and blanks is a form nobody filled in.
const UNFILLED: &str = "[context param]";

/// Why a title or a body says nothing. The set is closed: a writer that
/// answers differently for one of these matches on it, and the words a person
/// reads come from [`Reason::as_str`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Reason {
    /// Nothing was given.
    EmptyTitle,
    /// Every word is a label and nothing else.
    OnlyPrefix,
    /// One word, written more than once, with no label among the words.
    Repeated,
    /// The body holds both marks of the planner's brief.
    Briefed,
    /// The body holds nothing but the brief's unfilled markers, or nothing.
    Unfilled,
}

impl Reason {
    /// Every reason, in the order the rules ask.
    pub const ALL: [Reason; 5] = [
        Reason::EmptyTitle,
        Reason::OnlyPrefix,
        Reason::Repeated,
        Reason::Briefed,
        Reason::Unfilled,
    ];

    /// The title's refusals, in the order `check_title` asks them, so a writer
    /// that answers only for titles can enumerate them.
    pub const TITLE: [Reason; 3] = [Reason::EmptyTitle, Reason::OnlyPrefix, Reason::Repeated];

    /// What the person reading the comment is told.
    pub fn as_str(self) -> &'static str {
        match self {
            Reason::EmptyTitle => "the title is empty",
            // Plural: `EPIC: EPIC` is one prefix and `feat: fix:` is two, and
            // both are refused here.
            Reason::OnlyPrefix => "the title is nothing but prefixes",
            Reason::Repeated => "the title is one word repeated",
            Reason::Briefed => "the body is the planner's brief: it holds both `Descriptor:` and a `Prior context inventory`",
            Reason::Unfilled => "the body is nothing but `[context param]` markers",
        }
    }

    /// The line for the comment: the reason, with as much of the refused
    /// title as fits after it. The whole line is one line, and no longer than
    /// [`MAX_REASON_CHARS`] characters. A body is not repeated: the log holds
    /// it, and the comment only has to say which rule the body failed.
    fn line(self, title: &[String]) -> String {
        let mut said = String::from(self.as_str());
        let keep = MAX_REASON_CHARS
            .saturating_sub(said.chars().count())
            .saturating_sub(2);
        if !title.is_empty() && keep > 0 {
            said.push_str(": ");
            // Quoted the way a person reads it: its words, on one line.
            said.extend(title.join(" ").chars().take(keep));
        }
        said
    }
}

impl fmt::Display for Reason {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// The line a writer puts in the comment: the reason, and, of a title, as
/// much of it as fits on one line.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Refusal {
    /// Why the text was refused.
    pub reason: Reason,
    /// What the person reads: the reason, with the refused title after it
    /// when there is room for it. One line, of at most [`MAX_REASON_CHARS`]
    /// characters.
    pub line: String,
}

impl Refusal {
    /// The refusal of the title `read` for `reason`.
    fn of(reason: Reason, read: &[String]) -> Refusal {
        Refusal {
            reason,
            line: reason.line(read),
        }
    }

    /// The refusal of a body, whose own text is not repeated.
    fn of_body(reason: Reason) -> Refusal {
        Refusal {
            reason,
            line: reason.line(&[]),
        }
    }
}

impl fmt::Display for Refusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.line)
    }
}

/// `text` with the characters no reader sees taken out, so an invisible one
/// cannot keep a rule from seeing the word or mark around it: a control
/// character or a format character of Unicode. Whitespace is kept, because the
/// lines of a body are read by their newlines and the words of a text by their
/// spaces; a rule that reads one word where the text shows two is `bare`. A
/// carriage return goes, because a body of them alone is a body of nothing.
fn readable(text: &str) -> String {
    text.chars()
        .filter(|c| !(c.is_control() && *c != '\n' && *c != '\r' || invisible(*c)))
        .collect()
}

/// `c` as one letter of a word or one character of one of the brief's marks:
/// lower case, and with the punctuation a model swaps for another read as the
/// character it stands for — the fullwidth colon is the colon of `Descriptor:`,
/// which is the one a model has used to hide the field.
fn shape(c: char) -> char {
    match c {
        '\u{ff1a}' => ':',
        _ => c.to_ascii_lowercase(),
    }
}

/// The invisible characters. The mask's list of them is behind the `mask`
/// feature and this rule is not, so the range it guards is repeated here
/// rather than pulling the feature onto this module.
fn invisible(c: char) -> bool {
    matches!(
        c,
        '\u{ad}'
            | '\u{600}'..='\u{605}'
            | '\u{61c}'
            | '\u{6dd}'
            | '\u{70f}'
            | '\u{890}'..='\u{891}'
            | '\u{8e2}'
            | '\u{180e}'
            | '\u{200b}'..='\u{200f}'
            | '\u{202a}'..='\u{202e}'
            | '\u{2060}'..='\u{2064}'
            | '\u{2066}'..='\u{206f}'
            | '\u{feff}'
            | '\u{fff9}'..='\u{fffb}'
            | '\u{110bd}'
            | '\u{110cd}'
            | '\u{13430}'..='\u{1343f}'
            | '\u{1bca0}'..='\u{1bca3}'
            | '\u{1d173}'..='\u{1d17a}'
            | '\u{e0001}'
            | '\u{e0020}'..='\u{e007f}'
    )
}

/// Whether `line` opens with `mark`, both read without case, without the
/// punctuation around them, and without the space between their words: the
/// field and the heading of the brief, however the model laid them out or
/// spelled their colon.
fn says(text: &str, mark: &str) -> bool {
    let need = bare(mark);
    if need.is_empty() {
        return true;
    }
    // The mark read out of the whole text, which is how a reader of one line of
    // a body reads it: the newline a model put in the middle of a word of the
    // mark, or the character it glued in there, is not a reason for the mark to
    // go unread.
    bare(text).starts_with(need.as_str())
}

/// The spaces a model puts in the middle of a word or a mark, where a reader
/// would still read one of them.
fn looks_like_space(c: char) -> bool {
    matches!(
        c,
        '\u{a0}' | '\u{1680}' | '\u{2000}'..='\u{200a}' | '\u{202f}' | '\u{205f}' | '\u{3000}'
    )
}

/// `text` as one of the brief's marks is written: lower case, with the colon
/// of `Descriptor:` read as the colon it stands for, and with every character
/// that is neither a letter nor a digit taken out, so neither the punctuation
/// of a markdown heading nor a look-alike colon keeps a mark from being read.
fn bare(text: &str) -> String {
    text.chars()
        .filter(|c| !invisible(*c))
        .map(shape)
        .filter(|c| c.is_alphanumeric())
        .collect()
}

/// `title` as one line of words, which is what both of its rules compare:
/// lower case, and with the punctuation at either end of a word taken off, so
/// `EPIC:` and `epic` are one word, and a word left with nothing is not a
/// word. A word written over two lines is the word written twice, which is
/// what the rule of a repeated word asks, so the newline changes nothing and
/// `EPIC: EPIC` on two lines is the title `EPIC EPIC` on one. An invisible
/// character between two letters does the same, and `_` stands for a space in
/// a name, so `check_body` is the two words `check` and `body`.
fn read_title(title: &str) -> Vec<String> {
    // The title is read across its lines, because a title is one line on the
    // screen of an issue: a writer who broke a word over two of them wrote the
    // word once, and cannot hide one word of a repeated title behind the
    // newline between them. The characters no reader sees go first, so an
    // invisible one glued into a word, a control character, or a space a model
    // writes inside a word all leave the word a reader reads. Then every
    // character that is neither a letter nor a digit splits a word, which takes
    // the punctuation off its ends; `_` stands for a space in a name, so
    // `check_body` is the two words `check` and `body`.
    // The title read as one line first: every character that separates words,
    // the newline of a title written over two lines among them, becomes one
    // space, so a title cannot be made to pass by breaking its words over lines.
    let said: String = title
        .chars()
        .filter(|c| !invisible(*c) && !looks_like_space(*c))
        .collect();
    let mut read: Vec<String> = Vec::new();
    // Every character that is neither a letter nor a digit ends a word: the
    // punctuation at its ends, and the newline of a title written over two
    // lines. A reader of the title of #1015 reads `wo` and `rk` where a writer
    // broke `work` in two, and reads `work` twice where a writer wrote the one
    // word on two lines; a newline separates words, it never joins them.
    for part in said.split(char::is_whitespace) {
        for word in part.split(|c: char| !(c == '_' || c.is_alphanumeric())) {
            let word = word.chars().map(shape).collect::<String>();
            let word = word.trim_matches('_');
            if !word.is_empty() {
                read.push(word.to_owned());
            }
        }
    }
    read
}

/// Whether every letter of `word` is one and the same letter. A model that
/// ran out of what to say filled the body of an issue with one letter written
/// over and over; it is a run of letters and holds no word of an issue.
fn all_one_letter(word: &str) -> bool {
    let mut letters = word.chars();
    let Some(first) = letters.next() else {
        return true;
    };
    letters.all(|c| c == first)
}

/// A word of a title is a label and nothing more.
fn is_label(word: &str) -> bool {
    LABELS.contains(&word)
}

/// Whether the words of a title say one thing twice. A single word is a
/// title on its own, so it takes more than one word to say nothing.
fn is_repeated(words: &[String]) -> bool {
    words.len() > 1 && words.iter().all(|w| w == &words[0])
}

/// A title that says what the issue is, or the reason it does not.
///
/// `body` is the body the title is published with. No rule here reads it: the
/// title repeating the body's words is one of the two rules #1015 leaves out
/// (see the module's header), and only the writer that holds the planner's
/// inventory can answer it. The parameter stays so that rule is added inside
/// this signature rather than by changing every call.
#[must_use = "a refused title must stop the write"]
pub fn check_title(title: &str, _body: &str) -> Result<(), Refusal> {
    let read = read_title(title);

    // Nothing was given, or so much that it cannot be a summary of anything:
    // the same reason, because neither title says what the issue is. A title
    // too long to have been read quotes nothing, as a body never quotes.
    if read.is_empty() {
        return Err(Refusal::of(Reason::EmptyTitle, &read));
    }
    if title.chars().count() > MAX_TITLE_CHARS {
        return Err(Refusal::of(Reason::EmptyTitle, &[]));
    }
    if read.iter().all(|word| is_label(word)) {
        return Err(Refusal::of(Reason::OnlyPrefix, &read));
    }

    // A word of its own written more than once: a single word is a title on
    // its own, so only a word written twice says nothing. A title of only
    // labels is the prefix rule above, which is the truer reason and is asked
    // first, so membership in LABELS decides which of the two a text gets.
    if is_repeated(&read) {
        return Err(Refusal::of(Reason::Repeated, &read));
    }

    Ok(())
}

/// A body that holds the issue, or the reason it does not.
#[must_use = "a refused body must stop the write"]
pub fn check_body(body: &str) -> Result<(), Refusal> {
    // The text first read without the characters no reader sees; then, past the
    // bound, refused before any of it is examined, so its cost is the bound's
    // and never whatever a turn of the model emitted.
    let said = readable(body);
    if body.chars().count() > MAX_BODY_CHARS {
        // So much that it cannot be the issue: said as the brief is, because
        // none of it is read.
        return Err(Refusal::of_body(Reason::Briefed));
    }

    // The two marks of the planner's brief, which together are what #898
    // published as its issue. Neither mark alone is the brief: `Descriptor:` on
    // its own is a field a body may well hold, and so is a section that invents
    // prior context. Each is read at the head of a text taken out of the body.
    // A body is read three ways: line by line, which is how the form writes the
    // two marks; in blocks, which is the same form with a blank line between its
    // two parts; and as one text with the ends of its lines taken out, because a
    // model that echoed the form back broke its heading over two lines as
    // readily as it wrote one whole and a reader sees the same phrase either way.
    // A body that only quotes the two marks in one sentence of its own meets
    // none of the three readings: the marks are then in the middle of what is
    // read, and that body says what the issue is, which is what the rule is
    // meant to let stand.
    let one = said.replace('\n', "");
    let opens = |l: &str, mark: &str| says(&l.replace('\n', ""), mark);
    let the_form = |mark: &str| {
        said.lines().any(|l| opens(l, mark))
            || said.split("\n\n").map(str::trim).any(|l| opens(l, mark))
            || says(&one, mark)
    };
    if the_form(BRIEF_FIELD) && the_form(BRIEF_INVENTORY) {
        return Err(Refusal::of_body(Reason::Briefed));
    }

    // Nothing of the issue. The body as a reader of it reads a line: with the
    // blanks of the form gone, what is left is either words of the issue or
    // nothing. The marker `[context param]` is a marker however it is padded
    // with characters no reader sees, and a body of one character written over
    // and over — `a` repeated a thousand times is what a model that ran out of
    // what to say emits — holds no word of an issue.
    let left: Vec<String> = said
        .split(['\n', '\r'])
        .map(|l| l.trim().to_owned())
        .collect();
    let words = |l: &str| {
        let written = l != UNFILLED;
        let runs: Vec<String> = l
            .split(|c: char| !(c == '_' || c.is_alphanumeric()))
            .map(|w| w.to_ascii_lowercase())
            .filter(|w| !w.is_empty())
            .collect();
        written && runs.len() > 1 && !is_repeated(&runs) && runs.iter().any(|w| !all_one_letter(w))
    };
    if !left.iter().map(String::as_str).any(words) {
        return Err(Refusal::of_body(Reason::Unfilled));
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn refused_title(title: &str) -> Reason {
        match check_title(title, "") {
            Ok(()) => panic!("expected a refusal of {title:?}"),
            Err(r) => r.reason,
        }
    }

    fn refused_body(body: &str) -> Reason {
        match check_body(body) {
            Ok(()) => panic!("expected a refusal of {body:?}"),
            Err(r) => r.reason,
        }
    }

    #[test]
    fn a_title_of_only_its_prefix_is_refused() {
        assert_eq!(refused_title("EPIC: EPIC"), Reason::OnlyPrefix);
        for only in [
            "epic:",
            "EPIC:",
            "Epic",
            "feat:",
            "Fix:",
            "chore: chore:",
            "docs: docs: docs:",
        ] {
            assert_eq!(refused_title(only), Reason::OnlyPrefix, "{only:?}");
        }
        // A summary after the prefix makes it a title, and the list is the
        // list and no more: `milestone:` is no label of tehas's, so a title
        // of nothing but it is caught as one word repeated, not as a prefix.
        assert!(check_title("EPIC: Issue titles must summarise the issue", "").is_ok());
        assert!(check_title("feat(work): the console reads the new key", "").is_ok());
        assert_eq!(refused_title("milestone: milestone"), Reason::Repeated);
        assert!(check_title("milestone: the console reads it", "").is_ok());
    }

    #[test]
    fn a_title_of_one_word_repeated_is_refused() {
        assert_eq!(refused_title("Fast fast"), Reason::Repeated);
        for again in ["work work", "Work  WORK", "ready? ready?", "ab ab ab"] {
            assert_eq!(refused_title(again), Reason::Repeated, "{again:?}");
        }
        assert!(check_title("Fast work", "").is_ok());
    }

    #[test]
    fn a_plain_short_title_passes() {
        assert!(check_title("Fast", "").is_ok());
        assert!(check_title("The console reads the new key", "").is_ok());
        // An empty title says nothing, as a prefix alone does, and so does a
        // title whose every word folds to nothing.
        assert_eq!(refused_title(""), Reason::EmptyTitle);
        assert_eq!(refused_title("   "), Reason::EmptyTitle);
        assert_eq!(refused_title("--- ?? !!"), Reason::EmptyTitle);
    }

    #[test]
    fn the_echoed_brief_is_refused() {
        let brief = "Descriptor: what the issue is\n\n## Prior context inventory\n\n- the file";
        assert_eq!(refused_body(brief), Reason::Briefed);
        // Neither mark alone is the brief: a body may hold either in good
        // faith, and order, case and the lines they are written over do not
        // matter.
        assert_eq!(
            refused_body("Prior Context Inventory\nDescriptor: x"),
            Reason::Briefed
        );
        assert!(check_body("Descriptor: what we ship\n\nThe console reads it.").is_ok());
        assert!(check_body("## Prior context inventory\n\nThe gate is green.").is_ok());
        // A body that only mentions both in one sentence of its own says what
        // the issue is, and is the module's own doc read as a body.
        assert!(check_body(
            "A body that names a Descriptor: and a Prior context inventory in one sentence is talked about here."
        )
        .is_ok());
    }

    #[test]
    fn a_body_of_only_param_markers_is_refused() {
        assert_eq!(
            refused_body("[context param]\n\n  [context param]  \n"),
            Reason::Unfilled
        );
        // An empty body has nothing of the issue either, as a body of markers
        // has not, and both are said the same way: the rule is that no line
        // holds text.
        assert_eq!(refused_body(""), Reason::Unfilled);
        assert_eq!(refused_body("\r"), Reason::Unfilled);
        // One line of the issue's own text makes it a body.
        assert!(check_body("[context param]\n\nThe console reads the new key.").is_ok());
    }

    // --- the shapes that used to pass unseen ---------------------------------

    #[test]
    fn a_newline_in_the_middle_of_a_text_does_not_hide_a_rule() {
        // The newline in the middle of a title separates words and never joins
        // them, so the word written twice over its two lines is the word
        // written twice, and a title cannot be made to pass by laying its words
        // over two lines.
        assert_eq!(refused_title("work\nwork"), Reason::Repeated);
        // And a title of nothing but its prefix is the prefix rule, whichever
        // line the second prefix was written on.
        assert_eq!(refused_title("EPIC:\nEPIC"), Reason::OnlyPrefix);
        // A model that echoed the form back wrote its blocks with the lines
        // broken where they came out, so the marks are read out of the whole
        // body as well as out of its lines: the heading of the brief below runs
        // over two of them, and the body is the brief of #898.
        for body in [
            "Descriptor:\n\nprior context\ninventory",
            "Descriptor\u{ff1a} x\nprior context inventory",
        ] {
            assert_eq!(refused_body(body), Reason::Briefed, "{body:?}");
        }
        // Neither mark on its own is the form, however the model broke its
        // lines, and a body that quotes the two marks in a sentence of its own
        // says what the issue is.
        assert!(check_body("prior context\ninventory\n\nThe console reads it.").is_ok());
        assert!(check_body(
            "A body that names a Descriptor: and a Prior context inventory in one sentence is talked about here."
        )
        .is_ok());
    }

    #[test]
    fn an_invisible_character_does_not_hide_a_rule() {
        // A zero-width space inside a word, a word-joiner inside a prefix,
        // and a control character glued to a marker: none of them is a
        // reason to publish what they hide.
        assert_eq!(refused_title("wo\u{200b}rk work"), Reason::Repeated);
        assert_eq!(refused_title("EPIC\u{2060}: EPIC"), Reason::OnlyPrefix);
        assert_eq!(refused_title("work\u{0} work"), Reason::Repeated);
        assert_eq!(refused_body("\u{200b}\u{200b}"), Reason::Unfilled);
        assert_eq!(refused_body("\u{2060}"), Reason::Unfilled);
        assert_eq!(refused_body("[context param]\u{200b}\n"), Reason::Unfilled);
        assert_eq!(refused_body("[context param]\u{0}"), Reason::Unfilled);
    }

    #[test]
    fn a_look_alike_punctuation_does_not_hide_a_mark() {
        // The fullwidth colon is no punctuation to the word rules and no colon
        // to a phrase, so the field is matched without its punctuation.
        assert_eq!(
            refused_body("Descriptor\u{ff1a} x\nprior\u{200b} context inventory"),
            Reason::Briefed
        );
        assert_eq!(
            refused_body("Descriptor\u{ff1a} what the issue is\nprior context inventory"),
            Reason::Briefed
        );
        // A body that holds the field and nothing of the issue still fails
        // the other rule, not this one.
        assert_eq!(refused_body("Descriptor\u{ff1a}"), Reason::Unfilled);
    }

    #[test]
    fn a_title_past_its_bound_is_refused_without_being_read() {
        // Neither an unreadable title nor an empty one says what the issue
        // is, so both are said the same way.
        assert_eq!(refused_title(""), Reason::EmptyTitle);
        assert_eq!(refused_title("   "), Reason::EmptyTitle);
        assert_eq!(refused_title("--- ?? !!"), Reason::EmptyTitle);
        assert_eq!(
            refused_title(&"work ".repeat(1_000_000)),
            Reason::EmptyTitle
        );
        // The line of the title that was read quotes its words, on one line and
        // cut short; the line of one too long to read quotes nothing.
        let said = check_title("work work", "").expect_err("expected a refusal");
        assert_eq!(said.line, "the title is one word repeated: work work");
        let said = check_title(&("work ".repeat(100) + "work"), "").expect_err("a refusal");
        assert!(
            said.line.chars().count() <= MAX_REASON_CHARS,
            "{}",
            said.line
        );
        assert!(!said.line.contains('\n'));
        let said = check_title(&"work ".repeat(1_000_000), "").expect_err("a refusal");
        assert_eq!(said.reason, Reason::EmptyTitle);
        assert_eq!(said.line, Reason::EmptyTitle.as_str());
    }

    #[test]
    fn the_line_of_a_body_names_no_text_of_its_own() {
        // A refusal says which rule the body failed and repeats none of it:
        // the log holds the body, and the comment is read by a person.
        // A body of nothing, and a form nobody filled in, hold nothing of the
        // issue.
        for body in [
            String::new(),
            "[context param]\n\n  [context param]  \n".to_string(),
        ] {
            let said = check_body(&body).expect_err("expected a refusal");
            assert_eq!(said.line, Reason::Unfilled.as_str());
        }
        // A body of one long word of its own holds the issue no more than a form
        // of nothing does, and a body of the two words of the form's marker does
        // hold it, so the rule asks for two runs of letters, not for length.
        for body in ["x".repeat(500), "y".repeat(5000)] {
            let said = check_body(&body).expect_err("expected a refusal");
            assert_eq!(said.line, Reason::Unfilled.as_str());
        }
        for body in [
            "Descriptor: what the issue is\n\n## Prior context inventory\n\n- the file".to_string(),
            "Prior Context Inventory\nDescriptor: x".to_string(),
            "a".repeat(10_000_000),
        ] {
            let said = check_body(&body).expect_err("expected a refusal");
            assert_eq!(said.line, Reason::Briefed.as_str());
        }
    }

    #[test]
    fn a_text_past_the_bound_is_refused_not_examined() {
        // Past the bound a check stops, so its cost is the bound's and not
        // whatever a turn of the model emitted.
        assert_eq!(
            refused_title(&"work ".repeat(1_000_000)),
            Reason::EmptyTitle
        );
        assert_eq!(refused_body(&"a".repeat(10_000_000)), Reason::Briefed);
        // Within it, the same rules answer as ever.
        assert!(check_title(&("work ".repeat(100) + "the console"), "").is_ok());
        assert!(check_body(&"the console reads the new key ".repeat(100)).is_ok());
    }

    #[test]
    fn the_labels_are_the_list_and_no_more() {
        assert_eq!(
            LABELS,
            ["epic", "feat", "fix", "chore", "docs", "test", "refactor", "research"]
        );
    }

    #[test]
    fn every_reason_has_words_of_its_own() {
        let mut said: Vec<&str> = Reason::ALL.iter().map(|r| r.as_str()).collect();
        said.sort_unstable();
        said.dedup();
        assert_eq!(said.len(), Reason::ALL.len());
        // The title's refusals are the first three, so a writer that answers
        // only for titles can enumerate them.
        assert_eq!(
            Reason::TITLE,
            [Reason::EmptyTitle, Reason::OnlyPrefix, Reason::Repeated]
        );
    }

    #[test]
    fn the_message_of_a_rule_names_the_mark_the_rule_looks_for() {
        // The consts and the sentences move together: a rename that leaves
        // the message behind fails here rather than on the issue.
        let brief = Reason::Briefed.to_string();
        assert!(brief.contains("Descriptor:"), "{brief}");
        assert!(brief.contains("Prior context inventory"), "{brief}");
        assert!(Reason::Unfilled.to_string().contains(UNFILLED));
    }
}
