//! The images an issue links, fetched by tehas work for the cycle, and their
//! descriptions.

mod work;

use std::fs;
use std::path::Path;
use work::{
    count_of, has, lacks, security_of, unknown_options, World, A, DOCKER_OPTS, F, GIF, JPG, PNG,
    RUNNER, WANT, WEBP,
};

const CHAT: &str = "https://models.test/v1/chat/completions";

fn world(name: &str) -> World {
    let w = World::new(name);
    w.stub_github();
    w
}

fn sorted(mut v: Vec<String>) -> Vec<String> {
    v.sort();
    v
}

/// The names of `NAME=value` words.
fn names(words: Vec<String>) -> Vec<String> {
    words
        .into_iter()
        .map(|e| e.split_once('=').map_or(e.clone(), |(n, _)| n.to_string()))
        .collect()
}

fn lines(text: &str) -> Vec<String> {
    text.lines().map(String::from).collect()
}

/// The mounts of a cycle given nothing, and these besides.
fn mounts(w: &World, more: &[String]) -> Vec<String> {
    let mut v = lines(&w.mounts_off());
    v.extend_from_slice(more);
    sorted(v)
}

/// The variables of a cycle, by name, and these besides.
fn want(more: &[&str]) -> Vec<String> {
    let mut v = lines(WANT);
    v.extend(more.iter().map(|s| s.to_string()));
    sorted(v)
}

fn images_mount(w: &World) -> String {
    format!("{}:{}.images:ro", w.imgs.display(), w.inside.display())
}

fn images_var(w: &World) -> String {
    format!("PI_ENSEMBLE_EXTRA_IMAGE_DIRS={}.images", w.inside.display())
}

/// The `-e` words of the cycle that name the images' variable.
fn image_vars(w: &World) -> Vec<String> {
    w.after("-e")
        .into_iter()
        .filter(|e| e.starts_with("PI_ENSEMBLE_EXTRA_IMAGE_DIRS"))
        .collect()
}

/// The entries under `dir` as `find . -mindepth 1` names them, sorted.
fn found(dir: &Path) -> String {
    fn walk(dir: &Path, rel: &str, out: &mut Vec<String>) {
        for e in fs::read_dir(dir).unwrap().flatten() {
            let n = format!("{rel}/{}", e.file_name().to_string_lossy());
            if e.path().is_dir() {
                walk(&e.path(), &n, out);
            }
            out.push(n);
        }
    }
    let mut out = vec![];
    walk(dir, ".", &mut out);
    out.sort();
    out.iter().map(|n| format!("{n} ")).collect()
}

fn docker_runs(w: &World) -> usize {
    fs::read_dir(w.path("calls"))
        .unwrap()
        .flatten()
        .filter(|e| e.file_name().to_string_lossy().starts_with("docker.args."))
        .count()
}

/// Every word of every docker run, one per line.
fn all_docker_args(w: &World) -> String {
    let mut s = String::new();
    for e in fs::read_dir(w.path("calls")).unwrap().flatten() {
        if e.file_name().to_string_lossy().starts_with("docker.args.") {
            s.push_str(&fs::read_to_string(e.path()).unwrap());
        }
    }
    s
}

fn cycle_line(w: &World) -> String {
    w.cycle_args().join(" ")
}

/// The options the cycle's docker run may carry.
fn no_unknown_options(w: &World) {
    let valued = format!("--name {DOCKER_OPTS} --user --cidfile -w -v -e");
    assert_eq!(
        unknown_options(&w.cycle_args(), &w.image, &valued, "--rm -i --init"),
        Vec::<String>::new()
    );
    assert_eq!(count_of(&w.cycle_args(), "--tmpfs"), 1);
}

/// The lines of a file as `tr -d '\r' | LC_ALL=C sort | uniq -c` counts them,
/// each `<count> <line>|`.
fn uniq_counted(text: &str) -> String {
    let mut v: Vec<String> = text.lines().map(|l| l.replace('\r', "")).collect();
    v.sort();
    let mut out = String::new();
    let mut i = 0;
    while i < v.len() {
        let n = v[i..].iter().take_while(|l| **l == v[i]).count();
        out.push_str(&format!("{n} {}|", v[i]));
        i += n;
    }
    out
}

#[test]
fn with_images_off_github_is_not_asked_nothing_is_fetched_and_no_directory_is_made() {
    let w = world("images-off");
    w.images_on(&[]);
    w.minimal(&[]);
    w.issue_is(&format!("![shot]({A}/1111)"), "", "");
    w.net(&format!("{A}/1111"), PNG);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(format!("{}{}", w.gh(), w.calls("curl")), "");
    assert!(!w.imgs.exists());
    assert_eq!(sorted(w.after("-v")), mounts(&w, &[]));
    assert_eq!(sorted(names(w.after("-e"))), want(&[]));
}

#[test]
fn a_setting_that_is_not_0_or_1_starts_nothing_and_asks_nobody() {
    let w = world("images-bad");
    w.images_on(&[]);
    w.minimal(&["TEHAS_WORK_IMAGES=yes"]);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    assert_eq!(
        format!("{}{}{}", w.gh(), w.calls("curl"), w.calls("docker")),
        ""
    );
    assert!(!w.home.join("work").exists());
}

/// A start with images on, of an issue that links each kind GitHub keeps and
/// some it does not.
fn many(name: &str) -> World {
    let w = world(name);
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.net(&format!("{F}/21/side.JPG"), JPG);
    w.net(
        &format!("{A}/2222"),
        r#"<svg xmlns="http://www.w3.org/2000/svg"/>"#,
    );
    w.net(&format!("{F}/24/drawing.svg"), "<svg/>");
    w.net(&format!("{A}/3333"), GIF);
    w.net("https://user-images.githubusercontent.com/9/old.webp", WEBP);
    w.issue_is(
        &format!(
            "Look: ![shot]({A}/1111) and again ![shot]({A}/1111)
[side.JPG]({F}/21/side.JPG) [all.zip]({F}/23/all.zip) [drawing.svg]({F}/24/drawing.svg)
<img src=\"{A}/2222\"> ![anim]({A}/3333) https://user-images.githubusercontent.com/9/old.webp
and https://example.com/not-ours.png"
        ),
        "",
        "",
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    w
}

#[test]
fn with_images_on_the_cycle_gets_one_more_mount_read_only_and_one_more_variable() {
    let w = many("images-on");
    assert_eq!(sorted(w.after("-v")), mounts(&w, &[images_mount(&w)]));
    assert_eq!(
        sorted(names(w.after("-e"))),
        want(&["PI_ENSEMBLE_EXTRA_IMAGE_DIRS"])
    );
    assert_eq!(image_vars(&w), vec![images_var(&w)]);
    no_unknown_options(&w);
}

#[test]
fn the_push_checks_container_is_given_no_image_and_not_the_variable() {
    let w = many("images-push-check");
    let pre = w.preflight_args();
    assert_eq!(
        security_of(&pre, "-v"),
        vec![format!("{}:{}", w.ws.display(), w.inside.display())]
    );
    assert_eq!(sorted(names(security_of(&pre, "-e"))), want(&["HOME"]));
}

#[test]
fn nothing_is_made_on_the_host_at_the_images_path_inside_the_container() {
    let w = many("images-inside");
    let mut p = w.inside.clone().into_os_string();
    p.push(".images");
    assert!(!Path::new(&p).exists());
}

#[test]
fn png_jpg_gif_and_webp_are_taken_in_the_order_written_under_names_of_tehass_own() {
    let w = many("images-order");
    assert_eq!(
        found(&w.imgs),
        "./image-1.png ./image-2.jpg ./image-3.gif ./image-4.webp ./manifest.json "
    );
    assert_eq!(w.manifest(".issue"), "7");
    assert_eq!(
        w.manifest(".images | map([.file, .link, .type, .bytes])"),
        format!(
            "[[\"image-1.png\",\"{A}/1111\",\"image/png\",15],[\"image-2.jpg\",\"{F}/21/side.JPG\",\"image/jpeg\",11],[\"image-3.gif\",\"{A}/3333\",\"image/gif\",13],[\"image-4.webp\",\"https://user-images.githubusercontent.com/9/old.webp\",\"image/webp\",16]]"
        )
    );
    assert_eq!(fs::read(w.imgs.join("image-1.png")).unwrap(), PNG);
}

#[test]
fn another_type_is_left_out_and_said_and_one_known_by_its_name_is_not_fetched() {
    let w = many("images-left-out");
    assert_eq!(
        w.manifest(".leftOut | map([.link, .reason])"),
        format!(
            "[[\"{F}/23/all.zip\",\"Not a png, jpg, webp or gif image.\"],[\"{F}/24/drawing.svg\",\"Not a png, jpg, webp or gif image.\"],[\"{A}/2222\",\"Not a png, jpg, webp or gif image.\"]]"
        )
    );
    let curl = w.calls("curl");
    lacks(&curl, "all.zip");
    lacks(&curl, "drawing.svg");
    lacks(&curl, "example.com");
}

#[test]
fn the_token_goes_to_github_com_on_standard_input_never_on_the_command_line() {
    let w = many("images-token");
    lacks(&w.calls("curl"), "ghs_app");
    let headers = w.calls("curl-headers");
    has(&headers, &format!("{A}/1111 Authorization: Bearer ghs_app"));
    lacks(&headers, "githubusercontent");
}

#[test]
fn the_issue_is_told_how_many_images_the_cycle_was_given_and_what_was_left_out() {
    let w = many("images-told");
    has(
        &w.gh(),
        "issue comment 7 --repo acme/widget --body Tehas gave this cycle 4 images from the issue text.",
    );
    has(
        &w.gh(),
        &format!("- {F}/23/all.zip: Not a png, jpg, webp or gif image."),
    );
}

#[test]
fn the_limit_counts_all_images_together_each_fetch_may_take_what_is_left() {
    let w = many("images-limit");
    let re = regex::Regex::new(r"--max-filesize [0-9]*").unwrap();
    let got: String = re
        .find_iter(&w.calls("curl"))
        .map(|m| format!("{} ", m.as_str()))
        .collect();
    assert_eq!(
        got,
        "--max-filesize 10485760 --max-filesize 10485745 --max-filesize 10485734 --max-filesize 10485734 --max-filesize 10485721 "
    );
}

/// A start with images on of an issue that links one png.
fn one_png(w: &World, assoc: &str) {
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![shot]({A}/1111)"), assoc, "");
}

#[test]
fn an_image_the_sandbox_image_does_not_read_is_said_as_fetched_and_not_shown_and_still_mounted() {
    let w = world("images-not-read");
    w.images_on(&[]);
    w.reads_image("0");
    one_png(&w, "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert!(w.after("-v").contains(&images_mount(&w)));
    assert_eq!(image_vars(&w), vec![images_var(&w)]);
    lacks(&w.gh(), "Tehas gave this cycle");
    has(
        &w.gh(),
        "Tehas fetched 1 image from the issue text and mounted it read-only in the container of this cycle. The pi-rukas in the sandbox image does not read them, so no agent was shown one.",
    );
}

#[test]
fn a_sandbox_image_that_says_nothing_is_not_claimed_to_have_shown_the_images() {
    let w = world("images-unsaid");
    w.images_on(&[]);
    w.reads_image("none");
    one_png(&w, "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    lacks(&w.gh(), "Tehas gave this cycle");
    has(
        &w.gh(),
        &format!(
            "The sandbox image does not say whether its pi-rukas reads them: rebuild it with {}/sandbox/build.sh, which records the answer.",
            w.home.display()
        ),
    );
}

#[test]
fn a_sandbox_image_that_reads_them_is_said_to_have_given_them() {
    let w = world("images-read");
    w.images_on(&[]);
    w.reads_image("1");
    one_png(&w, "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &w.gh(),
        "issue comment 7 --repo acme/widget --body Tehas gave this cycle 1 image from the issue text.",
    );
}

#[test]
fn bytes_that_are_not_the_image_the_name_says_are_left_out() {
    let w = world("images-bytes");
    w.images_on(&[]);
    w.net(
        &format!("{F}/21/shot.png"),
        r#"<svg xmlns="http://www.w3.org/2000/svg"><script/></svg>"#,
    );
    w.net(&format!("{F}/22/empty.png"), "");
    w.issue_is(
        &format!("[shot.png]({F}/21/shot.png) [empty.png]({F}/22/empty.png)"),
        "",
        "",
    );
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest("[(.images | length), (.leftOut | map(.reason))]"),
        r#"[0,["Not a png, jpg, webp or gif image.","Not a png, jpg, webp or gif image."]]"#
    );
    assert_eq!(w.manifest(".linksUnavailable"), "null");
    assert_eq!(found(&w.imgs), "./manifest.json ");
}

#[test]
fn an_issue_with_no_image_still_gets_the_mount_with_an_empty_list_and_no_comment() {
    let w = world("images-none");
    w.images_on(&[]);
    w.issue_is("Words only.", "", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.manifest("[.images, .leftOut]"), "[[],[]]");
    assert_eq!(
        w.manifest(".linksUnavailable"),
        r#""The text links no file of GitHub's storage.""#
    );
    assert!(w.after("-v").contains(&images_mount(&w)));
    lacks(&w.gh(), "issue comment");
}

#[test]
fn an_issue_by_someone_outside_the_repository_is_given_no_image_and_nothing_is_fetched() {
    let w = world("images-outsider");
    w.images_on(&[]);
    one_png(&w, "NONE");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.calls("curl"), "");
    assert_eq!(
        w.manifest("[.images, (.leftOut | map([.link, .reason]))]"),
        format!(
            "[[],[[\"{A}/1111\",\"The issue was not opened by an owner, member or collaborator, and its label was not applied by an allowed account.\"]]]"
        )
    );
    has(
        &cycle_line(&w),
        &format!("{RUNNER} work 7 --timeout 360 --json"),
    );
}

#[test]
fn an_outsiders_issue_that_an_allowed_account_labelled_is_given_its_images() {
    let w = world("images-outsider-labelled");
    w.images_on(&[]);
    one_png(&w, "NONE");
    w.labelled_by("alice");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest("[(.images | map(.link)), .leftOut]"),
        format!("[[\"{A}/1111\"],[]]")
    );
    assert!(w.after("-v").contains(&images_mount(&w)));
    has(
        &w.gh(),
        "api repos/acme/widget/issues/7/events?per_page=100 --paginate",
    );
}

#[test]
fn one_labelled_last_by_an_account_outside_the_list_is_not_and_nothing_is_fetched() {
    let w = world("images-mallory");
    w.images_on(&[]);
    one_png(&w, "NONE");
    w.labelled_by("mallory");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(w.calls("curl"), "");
    assert_eq!(w.manifest(".images"), "[]");
}

#[test]
fn events_that_cannot_be_read_start_no_cycle_exit_4_said_on_the_issue() {
    let w = world("images-events-fail");
    w.images_on(&[]);
    w.issue_is(&format!("![shot]({A}/1111)"), "NONE", "");
    w.fx("events-fail", "");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    has(
        &r.out,
        "tehas work: the issue's label events could not be read for its images, not starting a cycle",
    );
    assert_eq!(docker_runs(&w), 0);
    assert_eq!(w.calls("curl"), "");
    has(
        &w.gh(),
        "issue comment 7 --repo acme/widget --body Tehas started no cycle: the issue's label events could not be read",
    );
}

#[test]
fn at_most_eight_images_the_ninth_is_left_out_and_not_fetched() {
    let w = world("images-nine");
    w.images_on(&[]);
    let mut body = String::new();
    for i in 1..=9 {
        w.net(&format!("{A}/000{i}"), PNG);
        body.push_str(&format!(" ![s]({A}/000{i})"));
    }
    w.issue_is(&body, "", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest("[(.images | length), (.leftOut | map([.link, .reason]))]"),
        format!("[8,[[\"{A}/0009\",\"More than 8 images.\"]]]")
    );
    lacks(&w.calls("curl"), &format!("{A}/0009"));
}

#[test]
fn an_image_over_what_is_left_of_the_limit_is_left_out_and_the_cycle_starts() {
    let w = world("images-big");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.net_rc(&format!("{A}/1111"), 63);
    w.issue_is(&format!("![big]({A}/1111)"), "", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        w.manifest(".leftOut | map(.reason)"),
        r#"["Over the 10 MB limit of an issue."]"#
    );
    has(
        &cycle_line(&w),
        &format!("{RUNNER} work 7 --timeout 360 --json"),
    );
}

#[test]
fn an_image_that_cannot_be_fetched_curl_rc_starts_no_cycle_exit_4_said_on_the_issue() {
    let w = world("images-fetch-fails");
    for rc in [22, 6, 28] {
        w.images_on(&[]);
        w.net(&format!("{A}/1111"), PNG);
        w.net(&format!("{A}/2222"), PNG);
        w.net_rc(&format!("{A}/2222"), rc);
        w.issue_is(&format!("![a]({A}/1111) ![b]({A}/2222)"), "", "");
        let r = w.work7();
        assert_eq!(r.rc, 4, "curl {rc}: {}", r.out);
        has(
            &r.out,
            &format!("tehas work: {A}/2222 could not be fetched, not starting a cycle"),
        );
        assert_eq!(docker_runs(&w), 0);
        assert!(!w.imgs.exists() && !w.ws.exists());
        has(
            &w.gh(),
            &format!(
                "issue comment 7 --repo acme/widget --body Tehas started no cycle: {A}/2222 could not be fetched"
            ),
        );
    }
}

#[test]
fn an_issue_github_does_not_hand_out_starts_no_cycle_and_what_the_last_cycle_left_stays_where_it_is(
) {
    let w = world("images-no-issue");
    w.images_on(&[]);
    fs::create_dir_all(&w.ws).unwrap();
    w.home_file("work/.tehas-workspaces", "");
    fs::write(w.ws.join("kept.txt"), "unpushed\n").unwrap();
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    assert_eq!(
        fs::read_to_string(w.ws.join("kept.txt")).unwrap(),
        "unpushed\n"
    );
    let mut kept = w.ws.clone().into_os_string();
    kept.push(".before-retry");
    assert!(!Path::new(&kept).exists());
    has(
        &r.out,
        "tehas work: the issue could not be read for its images, not starting a cycle",
    );
    assert_eq!(docker_runs(&w), 0);
    w.fx("issue.json", "not json\n");
    let r = w.work7();
    assert_eq!(r.rc, 4, "{}", r.out);
    assert_eq!(docker_runs(&w), 0);
}

#[test]
fn a_comment_that_cannot_be_posted_does_not_stop_the_cycle() {
    let w = world("images-comment-fails");
    w.images_on(&[]);
    one_png(&w, "");
    w.fx("comment-fails", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &cycle_line(&w),
        &format!("{RUNNER} work 7 --timeout 360 --json"),
    );
}

#[test]
fn the_images_are_remade_for_every_cycle_what_an_earlier_one_left_is_gone() {
    let w = world("images-remade");
    w.images_on(&[]);
    fs::create_dir_all(&w.imgs).unwrap();
    w.home_file("work/.tehas-workspaces", "");
    fs::write(w.imgs.join("image-9.png"), "old\n").unwrap();
    w.issue_is("Words only.", "", "");
    w.work7();
    assert_eq!(found(&w.imgs), "./manifest.json ");
}

#[test]
fn a_path_of_the_instances_own_for_the_variable_is_replaced_not_added_to() {
    let w = world("images-own-path");
    w.images_on(&["PI_ENSEMBLE_EXTRA_IMAGE_DIRS=/elsewhere"]);
    w.issue_is("Words only.", "", "");
    w.work7();
    assert_eq!(image_vars(&w), vec![images_var(&w)]);
    lacks(&all_docker_args(&w), "/elsewhere");
}

// --- The images described ----------------------------------------------------

fn describe_on(w: &World) {
    w.images_on(&["TEHAS_WORK_DESCRIBE=1"]);
    w.fs(
        "home/.pi/agent/models.json",
        "{\"providers\": {\"test-provider\": {\"baseUrl\": \"https://models.test/v1\"}}}\n",
    );
}

/// Two images and a model that describes each.
fn two_described(w: &World) {
    w.net(&format!("{A}/1111"), PNG);
    w.net(&format!("{A}/2222"), JPG);
    w.net(
        CHAT,
        r#"{"choices": [{"message": {"content": "A chart with three bars."}}]}"#,
    );
    w.issue_is(&format!("![a]({A}/1111) ![b]({A}/2222)"), "", "");
}

fn described(name: &str) -> World {
    let w = world(name);
    describe_on(&w);
    two_described(&w);
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    w
}

#[test]
fn a_description_setting_without_images_starts_nothing_exit_78() {
    let w = world("images-describe-alone");
    w.images_on(&[]);
    w.minimal(&["TEHAS_WORK_DESCRIBE=1"]);
    let r = w.work7();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "TEHAS_WORK_DESCRIBE is on and TEHAS_WORK_IMAGES is off",
    );
    assert_eq!(
        format!("{}{}{}", w.gh(), w.calls("curl"), w.calls("docker")),
        ""
    );
}

#[test]
fn with_descriptions_on_the_directory_holds_exactly_the_images_the_manifest_and_images_md() {
    let w = described("images-described");
    assert_eq!(
        found(&w.imgs),
        "./image-1.png ./image-2.jpg ./images.md ./manifest.json "
    );
    let md = fs::read_to_string(w.imgs.join("images.md")).unwrap();
    assert_eq!(
        md.lines()
            .filter(|l| *l == "> A chart with three bars.")
            .count(),
        2
    );
    assert_eq!(w.manifest(".described"), r#""images.md""#);
    assert_eq!(
        fs::read(w.home.join("state/cycles/7.images.md")).unwrap(),
        md.as_bytes()
    );
    let kept: serde_json::Value =
        serde_json::from_str(&fs::read_to_string(&w.kept).unwrap()).unwrap();
    assert_eq!(kept["described"], "images.md");
}

#[test]
fn with_descriptions_on_the_container_gets_the_same_mounts_and_variables_as_with_images_alone() {
    let w = described("images-described-mounts");
    assert_eq!(sorted(w.after("-v")), mounts(&w, &[images_mount(&w)]));
    assert_eq!(
        sorted(names(w.after("-e"))),
        want(&["PI_ENSEMBLE_EXTRA_IMAGE_DIRS"])
    );
    no_unknown_options(&w);
}

#[test]
fn the_model_is_asked_once_per_image_with_no_token_but_with_the_issues_number() {
    let w = described("images-described-asked");
    let curl = w.calls("curl");
    assert_eq!(curl.lines().filter(|l| l.contains(CHAT)).count(), 2);
    let headers = w.calls("curl-headers");
    let chat_headers: String = headers
        .lines()
        .filter(|l| l.contains(CHAT))
        .collect::<Vec<_>>()
        .join("\n");
    lacks(&chat_headers, "Authorization");
    // The issue's number goes in the config a model request hands curl on a
    // file descriptor, never on the command line.
    assert_eq!(curl.lines().filter(|l| l.contains("-K /proc/")).count(), 2);
    let config = w.calls("curl-config");
    assert_eq!(uniq_counted(&config), "2 header = \"X-Tehas-Issue: 7\"|");
    lacks(&curl, "X-Tehas-Issue");
    lacks(&config, "Authorization");
}

#[test]
fn with_descriptions_on_the_token_goes_in_the_config_beside_the_issue_and_neither_is_on_a_command_line(
) {
    let w = world("images-described-token");
    describe_on(&w);
    w.fs(
        "home/.pi/agent/models.json",
        "{\"providers\":{\"test-provider\":{\"baseUrl\":\"https://models.test/v1\",\"apiKey\":\"$TEHAS_MODEL_KEY_MAIN\"}}}\n",
    );
    two_described(&w);
    // The model's own key beside what a start is given.
    w.env("TEHAS_MODEL_KEY_MAIN", "main-secret");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    assert_eq!(
        uniq_counted(&w.calls("curl-config")),
        "2 header = \"Authorization: Bearer main-secret\"|2 header = \"X-Tehas-Issue: 7\"|"
    );
    lacks(&w.calls("curl"), "X-Tehas-Issue");
    lacks(&w.calls("curl"), "main-secret");
}

#[test]
fn a_model_that_does_not_answer_is_said_in_the_file_and_the_cycle_starts() {
    let w = world("images-model-silent");
    describe_on(&w);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![a]({A}/1111)"), "", "");
    let r = w.work7();
    assert_eq!(r.rc, 2, "{}", r.out);
    has(
        &fs::read_to_string(w.imgs.join("images.md")).unwrap(),
        "> not described: the request to the model failed (curl 22)",
    );
    has(
        &cycle_line(&w),
        &format!("{RUNNER} work 7 --timeout 360 --json"),
    );
}

#[test]
fn with_images_alone_nothing_is_described() {
    let w = world("images-alone");
    w.images_on(&[]);
    w.net(&format!("{A}/1111"), PNG);
    w.issue_is(&format!("![a]({A}/1111)"), "", "");
    w.work7();
    assert!(!w.imgs.join("images.md").exists());
    lacks(&w.calls("curl"), "chat/completions");
    assert_eq!(w.manifest(".described"), "null");
}
