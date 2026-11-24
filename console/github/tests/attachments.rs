//! `Blueprints::attachments_in` as the adapter answers it: the files a text
//! links to on GitHub's store, by tehas's one link rule, with no request.
//! Every case reads a text through a handle whose runner fails any call,
//! so a request would show.

use tehas_core::forge::GhRunner;
use tehas_github::GitHub;
use tehas_ports::{Attachment, BlueprintNo, Blueprints, Outcome, Refusal, WEB_LINK_MAX};

/// A runner that answers no call: the reading makes none.
struct NoCall;

impl GhRunner for NoCall {
    fn run(&mut self, args: &[&str]) -> Result<String, String> {
        panic!("a request was made: {args:?}")
    }
}

fn gh() -> GitHub {
    GitHub::new("acme/widget", Box::new(NoCall))
}

/// The id, link and name of each attachment of `text`.
fn read(text: &str) -> Vec<(String, String, Option<String>)> {
    gh().attachments_in(text)
        .into_iter()
        .map(|a: Attachment| {
            (
                a.id.as_str().to_string(),
                a.link.as_str().to_string(),
                a.name,
            )
        })
        .collect()
}

fn links(text: &str) -> Vec<String> {
    read(text).into_iter().map(|(_, link, _)| link).collect()
}

const FILE: &str = "https://github.com/user-attachments/files/31/plan.md";
const ASSET: &str =
    "https://github.com/user-attachments/assets/11111111-2222-3333-4444-555555555555";

#[test]
fn a_file_uploaded_to_an_issue_has_its_name_and_its_link_as_its_id() {
    assert_eq!(
        read(&format!("The plan: [plan.md]({FILE}).")),
        [(
            FILE.to_string(),
            FILE.to_string(),
            Some("plan.md".to_string())
        )]
    );
}

#[test]
fn a_pasted_image_has_no_name() {
    for link in [
        ASSET,
        "https://github.com/acme/widget/assets/42/0f1e2d3c-4b5a-6978-8a9b-0c1d2e3f4a5b",
        "https://user-images.githubusercontent.com/100001/12345-abcdef.png",
    ] {
        let got = read(&format!("![shot]({link})"));
        assert_eq!(got, [(link.to_string(), link.to_string(), None)], "{link}");
    }
}

#[test]
fn a_projects_uploaded_file_has_its_name() {
    let link = "https://github.com/acme/widget/files/77/notes.txt";
    assert_eq!(
        read(link),
        [(
            link.to_string(),
            link.to_string(),
            Some("notes.txt".to_string())
        )]
    );
}

#[test]
fn each_attachment_is_given_once_in_the_order_the_text_links_it() {
    let text = format!("{ASSET}\nthen {FILE}\nand {ASSET} again.\n");
    assert_eq!(links(&text), [ASSET, FILE]);
    // The text twice gives the same, as the ports' law says.
    assert_eq!(links(&format!("{text}\n{text}")), [ASSET, FILE]);
}

#[test]
fn a_text_with_no_link_has_no_attachment() {
    assert!(read("no link here").is_empty());
    assert!(read("").is_empty());
}

#[test]
fn a_link_of_another_host_or_scheme_or_with_userinfo_is_no_attachment() {
    for text in [
        "http://github.com/user-attachments/files/31/plan.md",
        "https://example.com/user-attachments/files/31/plan.md",
        "https://github.com.example.com/user-attachments/files/31/plan.md",
        "https://user@github.com/user-attachments/files/31/plan.md",
        "https://github.com@example.com/user-attachments/files/31/plan.md",
        "ftp://github.com/user-attachments/files/31/plan.md",
        "https://github.com/acme/widget/issues/7",
        "https://github.com/acme/widget/blob/main/README.md",
    ] {
        assert!(read(text).is_empty(), "{text}");
    }
}

#[test]
fn a_file_name_must_be_one_plain_part_of_the_path() {
    for text in [
        // No name.
        "https://github.com/user-attachments/files/31",
        "https://github.com/user-attachments/files/31/",
        // `.` and `..`.
        "https://github.com/user-attachments/files/31/.",
        "https://github.com/user-attachments/files/31/..",
        "https://github.com/user-attachments/files/31/../plan.md",
        // An escaped slash or dot, in either case: the rule cuts the link,
        // and a cut link is no attachment.
        "https://github.com/user-attachments/files/31/a%2fb.md",
        "https://github.com/user-attachments/files/31/a%2Fb.md",
        "https://github.com/user-attachments/files/31/%2e%2e",
        "https://github.com/user-attachments/files/31/a%5Cb.md",
        // More than one part after the number.
        "https://github.com/user-attachments/files/31/a/b.md",
        // A number that is no number.
        "https://github.com/user-attachments/files/x1/plan.md",
        "https://github.com/acme/widget/files/x1/plan.md",
    ] {
        assert!(read(text).is_empty(), "{text}: {:?}", read(text));
    }
}

#[test]
fn a_link_longer_than_a_web_link_holds_is_no_attachment() {
    let long = format!(
        "https://github.com/user-attachments/assets/{}",
        "a".repeat(WEB_LINK_MAX)
    );
    assert!(read(&long).is_empty());
    let fits = format!(
        "https://github.com/user-attachments/assets/{}",
        "a".repeat(WEB_LINK_MAX - ASSET.len() + 36)
    );
    assert_eq!(fits.chars().count(), WEB_LINK_MAX);
    assert_eq!(links(&fits), [fits]);
}

#[test]
fn a_link_in_code_or_a_quote_is_read_as_the_text_gives_it() {
    // A link in code or a quote counts, read as the text gives it, as
    // attach, the dispatcher and the issue feed read the raw text.
    for text in [
        format!("`{FILE}`"),
        format!("```\n{FILE}\n```"),
        format!("> {FILE}"),
    ] {
        assert_eq!(links(&text), [FILE], "{text}");
    }
}

#[test]
fn the_same_link_whole_elsewhere_in_the_text_is_an_attachment() {
    let text = format!("{FILE}/../x and {FILE}.");
    // The first is cut by the rule; the second stands whole. The trailing
    // dot is part of a name by the rule, so the whole one is `plan.md.`.
    assert_eq!(links(&text), [format!("{FILE}.")]);
    let text = format!("{FILE}/../x and {FILE} here");
    assert_eq!(links(&text), [FILE]);
}

#[test]
fn a_link_followed_straight_away_by_a_character_that_is_not_ascii_is_cut_and_no_attachment() {
    // The rule takes ASCII alone, so it ends this link before the `ä`: the
    // file is not `a.zip`.
    assert!(read("https://github.com/user-attachments/files/1/a.zipä").is_empty());
}

#[test]
fn a_link_then_a_space_then_a_word_that_is_not_ascii_is_an_attachment() {
    let link = "https://github.com/user-attachments/files/1/a.zip";
    assert_eq!(
        read(&format!("{link} är här")),
        [(
            link.to_string(),
            link.to_string(),
            Some("a.zip".to_string())
        )]
    );
}

#[test]
fn a_name_is_as_the_link_spells_it() {
    let link = "https://github.com/user-attachments/files/2/my%20file.pdf";
    assert_eq!(read(link)[0].2.as_deref(), Some("my%20file.pdf"));
}

#[test]
fn the_blueprints_attachments_are_not_built_and_say_so_before_any_request() {
    let no = BlueprintNo::new(7).unwrap();
    assert_eq!(
        gh().attachments(no).map(|_| ()),
        Err(Outcome::Refused(Refusal::Unsupported))
    );
}
