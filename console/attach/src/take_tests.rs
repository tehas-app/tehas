use crate::given::{Linked, Ordered};
use crate::sniff::Sniffer;
use crate::take::*;
use crate::unpack;
use crate::zip::Archive;
use serde_json::{json, Value};
use std::cell::RefCell;
use std::collections::HashMap;
use std::fs;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicUsize, Ordering};

const PNG: &[u8] = b"\x89PNG\r\n\x1a\nPNGDATA";

/// The network of a case: bytes by link, a curl status by link, and the real
/// sniff and unpack in this process.
#[derive(Default)]
struct Fake {
    bytes: HashMap<String, Vec<u8>>,
    rc: HashMap<String, i32>,
    unpack: Option<Failure2>,
    fetched: RefCell<Vec<(String, i64)>>,
}

#[derive(Clone, Copy)]
struct Failure2(&'static str);

impl Net for Fake {
    fn fetch(&self, link: &str, file: &str, most: i64) -> i32 {
        self.fetched.borrow_mut().push((link.to_string(), most));
        if let Some(rc) = self.rc.get(link) {
            return *rc;
        }
        let data = self.bytes.get(link).map_or(PNG, Vec::as_slice);
        if data.len() as i64 > most {
            return 63;
        }
        fs::write(file, data).unwrap();
        0
    }

    fn attach(&self, verb: &str, args: &[String]) -> Result<Value, Failure> {
        if verb == "sniff" {
            let mut s = Sniffer::default();
            s.write_all(&fs::read(&args[0]).unwrap()).unwrap();
            let s = s.finish();
            return Ok(
                json!({"bytes": s.bytes, "sha256": s.sha256, "image": s.image,
                "zip": s.zip, "text": s.text}),
            );
        }
        if let Some(Failure2(word)) = self.unpack {
            return Err(Failure {
                word,
                why: "attach: unpack exited 1: no".to_string(),
            });
        }
        let data = fs::read(&args[0]).unwrap();
        let archive = Archive::open(&data).map_err(|why| Failure {
            word: "unreadable",
            why: why.to_string(),
        })?;
        let (bytes, files) = (args[3].parse().unwrap(), args[5].parse().unwrap());
        let entries = unpack::unpack(&archive, Path::new(&args[1]), bytes, files).unwrap();
        Ok(json!({"entries": entries}))
    }
}

fn scratch() -> String {
    static N: AtomicUsize = AtomicUsize::new(0);
    let n = N.fetch_add(1, Ordering::SeqCst);
    let dir = std::env::temp_dir().join(format!("attach-take-{}-{n}", std::process::id()));
    let _ = fs::remove_dir_all(&dir);
    fs::create_dir_all(&dir).unwrap();
    dir.to_str().unwrap().to_string()
}

const ALL: Wants = Wants {
    images: true,
    files: true,
    zips: true,
};

fn take<'a>(net: &'a Fake, dir: &'a str, wants: Wants) -> Take<'a> {
    Take {
        net,
        dir,
        wants,
        approved: true,
        who: "",
        allowed: "alice bob",
        n: Counts::default(),
        images: Vec::new(),
        files: Vec::new(),
        left: Vec::new(),
    }
}

fn linked(source: &str, link: &str, by: &str) -> Linked {
    Linked {
        source: source.to_string(),
        link: link.to_string(),
        by: by.to_string(),
    }
}

fn body(link: &str) -> Linked {
    linked("body", link, "")
}

fn file(n: usize, name: &str) -> String {
    format!("https://github.com/user-attachments/files/{n}/{name}")
}

fn zip_of(entries: &[(&str, &[u8])]) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for (name, data) in entries {
        let crc = {
            let mut c = flate2::Crc::new();
            c.update(data);
            c.sum()
        };
        let at = out.len() as u32;
        let (size, len) = (data.len() as u32, name.len() as u16);
        out.extend_from_slice(b"PK\x03\x04\x14\0\0\0\0\0\0\0\0\0");
        out.extend_from_slice(&crc.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&size.to_le_bytes());
        out.extend_from_slice(&len.to_le_bytes());
        out.extend_from_slice(&[0, 0]);
        out.extend_from_slice(name.as_bytes());
        out.extend_from_slice(data);
        central.extend_from_slice(b"PK\x01\x02\x14\x03\x14\0\0\0\0\0\0\0\0\0");
        central.extend_from_slice(&crc.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&size.to_le_bytes());
        central.extend_from_slice(&len.to_le_bytes());
        central.extend_from_slice(&[0; 8]);
        central.extend_from_slice(&(0o100644u32 << 16).to_le_bytes());
        central.extend_from_slice(&at.to_le_bytes());
        central.extend_from_slice(name.as_bytes());
    }
    let start = out.len() as u32;
    out.extend_from_slice(&central);
    out.extend_from_slice(b"PK\x05\x06\0\0\0\0");
    let n = entries.len() as u16;
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&start.to_le_bytes());
    out.extend_from_slice(&[0, 0]);
    out
}

fn reasons(t: &Take) -> Vec<String> {
    t.left.iter().map(|e| e.shown("reason")).collect()
}

fn names(list: &[Ordered]) -> Vec<String> {
    list.iter().map(|e| e.shown("file")).collect()
}

fn listing(dir: &str) -> Vec<String> {
    let mut all: Vec<String> = fs::read_dir(dir)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .collect();
    all.sort();
    all
}

// --- Moved from test/attachments_test.sh --------------------------------------

#[test]
fn the_text_types_a_cycle_may_be_given_and_no_other() {
    for (ext, kind) in [
        ("md", "text/markdown"),
        ("txt", "text/plain"),
        ("log", "text/plain"),
        ("csv", "text/csv"),
        ("json", "application/json"),
        ("yaml", "application/yaml"),
        ("yml", "application/yaml"),
        ("html", "text/html"),
        ("css", "text/css"),
        ("js", "text/javascript"),
    ] {
        assert_eq!(text_type(ext), Some(kind));
    }
    for ext in ["svg", "htm", "mjs", "pdf", "zip", "png", ""] {
        assert_eq!(text_type(ext), None, "{ext}");
    }
}

#[test]
fn the_token_goes_to_github_com_only_and_on_standard_input() {
    let (args, with) = curl_args("https://github.com/a/b/assets/1/x", "/d/.part", 9, "tok");
    assert!(with);
    assert_eq!(
        args.join(" "),
        "-sSfL --proto =https --proto-redir =https --max-redirs 3 --max-time 120 --max-filesize 9 -o /d/.part -w %{content_type} -H @- https://github.com/a/b/assets/1/x"
    );
    assert!(!args.iter().any(|a| a.contains("tok")));
    let (args, with) = curl_args(
        "https://user-images.githubusercontent.com/9/x",
        "f",
        9,
        "tok",
    );
    assert!(!with && !args.contains(&"-H".to_string()));
    let (_, with) = curl_args("https://github.com.evil.example/x", "f", 9, "tok");
    assert!(!with);
    let (_, with) = curl_args("https://github.com/a/b/assets/1/x", "f", 9, "");
    assert!(!with);
}

#[test]
fn a_child_that_gives_no_answer_says_which_failure_as_the_shell_read_it() {
    let f = failed_for_test(1, b"attach: unreadable: /x: gone\nmore");
    assert_eq!(f.word, "unreadable");
    assert_eq!(
        f.why,
        "attach: sniff exited 1: attach: unreadable: /x: gone"
    );
    assert_eq!(
        failed_for_test(1, b"attach: unwritable: /x").word,
        "unwritable"
    );
    assert_eq!(failed_for_test(1, b"attach: other").word, "");
    assert_eq!(failed_for_test(124, b"").word, "timeout");
    assert_eq!(failed_for_test(137, b"").word, "timeout");
    assert_eq!(failed_for_test(101, b"panicked").word, "crashed");
    assert_eq!(failed_for_test(64, b"usage").word, "");
    let long = failed_for_test(64, &[b'\t'; 400]);
    assert_eq!(
        long.why,
        format!("attach: sniff exited 64: {}", "?".repeat(300))
    );
}

#[test]
fn a_program_that_is_not_there_is_absent() {
    let f = run_attach_for_test("/no/such/attach").unwrap_err();
    assert_eq!(f.word, "absent");
    assert_eq!(
        f.why,
        "attach: absent: /no/such/attach is not an executable file"
    );
}

// --- Names ---------------------------------------------------------------------

#[test]
fn a_link_is_named_by_its_last_part_and_a_pasted_one_has_no_name() {
    let of = |l: &str| link_ext(l);
    assert_eq!(of("https://github.com/user-attachments/assets/1.md"), None);
    assert_eq!(of("https://github.com/a/b/assets/1/x.md"), None);
    assert_eq!(of("https://github.com/a/b/files/1/x.MD"), Some("md".into()));
    assert_eq!(of("https://github.com/a/b/files/1/x"), Some(String::new()));
    assert_eq!(
        of("https://github.com/a/b/files/1/x.png?raw=1"),
        Some("png".into())
    );
    assert_eq!(
        of("https://github.com/a/b/files/1/x.png?a=/y.md"),
        Some("md".into())
    );
    // A host other than github.com has a name, whatever its path says.
    assert_eq!(
        of("https://user-images.githubusercontent.com/assets/x.gif"),
        Some("gif".into())
    );
}

#[test]
fn no_name_of_a_link_reaches_the_disk() {
    let dir = scratch();
    let evil = "https://github.com/user-attachments/files/1/..%2F..%2Fetc%2Fpasswd.md";
    let mut net = Fake::default();
    net.bytes.insert(evil.to_string(), b"text".to_vec());
    let mut t = take(&net, &dir, ALL);
    t.all(&[body(evil)]).unwrap();
    assert_eq!(names(&t.files), ["file-1.md"]);
    assert_eq!(t.files[0].shown("name"), "..%2F..%2Fetc%2Fpasswd.md");
    assert_eq!(listing(&dir), ["file-1.md"]);
}

#[test]
fn a_comments_author_is_held_to_whole_logins() {
    assert!(allowed_login("alice bob", "alice"));
    assert!(allowed_login("alice bob", "bob"));
    assert!(!allowed_login("alice bob", "ali"));
    assert!(!allowed_login("alice bob", "ice bob"));
    assert!(!allowed_login("alice bob", ""));
    assert!(!allowed_login("", ""));
    assert!(!allowed_login("", "alice"));
}

// --- Who may attach ------------------------------------------------------------

#[test]
fn who_may_attach() {
    let dir = scratch();
    let net = Fake::default();
    let png = file(1, "a.png");
    let outsider = |approved, who| {
        let mut t = take(&net, &dir, ALL);
        t.approved = approved;
        t.who = who;
        t.all(&[body(&png)]).unwrap();
        t.images.len()
    };
    assert_eq!(outsider(false, "NONE"), 0);
    assert_eq!(outsider(false, ""), 0);
    assert_eq!(outsider(true, "NONE"), 1);
    assert_eq!(outsider(false, "OWNER"), 1);
    assert_eq!(outsider(false, "MEMBER"), 1);
    assert_eq!(outsider(false, "COLLABORATOR"), 1);
    assert_eq!(outsider(false, "x:OWNER"), 0);

    let mut t = take(&net, &dir, ALL);
    t.approved = false;
    t.who = "OWNER";
    t.all(&[
        linked("answer:1", &png, ""),
        linked("comment:1", &png, "alice"),
    ])
    .unwrap();
    assert!(t.images.is_empty());
    assert_eq!(
        reasons(&t),
        [
            "The reply was not written by an allowed account.",
            "The comment was not written by an allowed account, or the start was not approved by one."
        ]
    );

    let mut t = take(&net, &dir, ALL);
    t.all(&[
        linked("comment:1", &png, "mallory"),
        linked("pr-comment:2", &png, "bob"),
    ])
    .unwrap();
    assert_eq!(t.images.len(), 1);
    assert_eq!(t.left.len(), 1);
    assert!(net.fetched.borrow().iter().all(|(l, _)| *l == png));
}

// --- Budgets and bytes ---------------------------------------------------------

#[test]
fn an_image_is_what_its_bytes_say_and_never_what_its_name_says() {
    let dir = scratch();
    let (liar, jpg, unsaid) = (file(1, "a.png"), file(2, "b.gif"), file(3, "c.png"));
    let mut net = Fake::default();
    net.bytes.insert(liar.clone(), b"<html>".to_vec());
    net.bytes.insert(jpg.clone(), b"\xff\xd8\xffJFIF".to_vec());
    net.bytes.insert(unsaid.clone(), PNG.to_vec());
    let mut t = take(
        &net,
        &dir,
        Wants {
            files: false,
            zips: false,
            images: true,
        },
    );
    t.all(&[body(&liar), body(&jpg), body(&unsaid)]).unwrap();
    assert_eq!(names(&t.images), ["image-1.jpg", "image-2.png"]);
    assert_eq!(t.images[0].shown("type"), "image/jpeg");
    assert_eq!(reasons(&t), ["Not a png, jpg, webp or gif image."]);
}

#[test]
fn at_most_eight_images_and_ten_mb() {
    let dir = scratch();
    let links: Vec<Linked> = (0..10).map(|i| body(&file(i, "a.png"))).collect();
    let net = Fake::default();
    let mut t = take(&net, &dir, ALL);
    t.all(&links).unwrap();
    assert_eq!(t.images.len(), 8);
    assert_eq!(reasons(&t), ["More than 8 images."; 2]);
    assert_eq!(net.fetched.borrow().len(), 8);

    let (a, b, c) = (file(1, "a.png"), file(2, "b.png"), file(3, "c.png"));
    let nine = [PNG, &[0u8; 9 * 1024 * 1024][..]].concat();
    let mut net = Fake::default();
    net.bytes.insert(a.clone(), nine.clone());
    net.bytes.insert(b.clone(), nine);
    let dir = scratch();
    let mut t = take(&net, &dir, ALL);
    t.all(&[body(&a), body(&b), body(&c)]).unwrap();
    assert_eq!(t.images.len(), 2);
    assert_eq!(reasons(&t), ["Over the 10 MB limit of an issue."]);
    assert_eq!(listing(&dir), ["image-1.png", "image-2.png"]);
}

#[test]
fn a_fetch_is_given_only_the_room_that_is_left() {
    let dir = scratch();
    let (md, zip) = (file(1, "a.md"), file(2, "z.zip"));
    let mut net = Fake::default();
    net.bytes.insert(md.clone(), b"hi".to_vec());
    net.bytes.insert(zip.clone(), zip_of(&[("b.md", b"x")]));
    let mut t = take(&net, &dir, ALL);
    t.n.fused = 3 * 1024 * 1024 + 512 * 1024;
    t.all(&[body(&md), body(&zip)]).unwrap();
    let room = 512 * 1024;
    assert_eq!(*net.fetched.borrow(), [(md, room), (zip, room - 2)]);
}

#[test]
fn at_most_sixteen_files_one_mb_each_and_four_in_all() {
    let dir = scratch();
    let links: Vec<Linked> = (0..18).map(|i| body(&file(i, "a.md"))).collect();
    let net_small = {
        let mut n = Fake::default();
        for l in &links {
            n.bytes.insert(l.link.clone(), b"hi".to_vec());
        }
        n
    };
    let mut t = take(&net_small, &dir, ALL);
    t.all(&links).unwrap();
    assert_eq!(t.files.len(), 16);
    assert_eq!(reasons(&t), ["More than 16 files."; 2]);

    let big = vec![b'x'; 1024 * 1024 + 1];
    let full = vec![b'y'; 1024 * 1024];
    let links: Vec<Linked> = (0..6).map(|i| body(&file(i, "a.md"))).collect();
    let mut net = Fake::default();
    net.bytes.insert(links[0].link.clone(), big);
    for l in &links[1..] {
        net.bytes.insert(l.link.clone(), full.clone());
    }
    let dir = scratch();
    let mut t = take(&net, &dir, ALL);
    t.all(&links).unwrap();
    assert_eq!(t.files.len(), 4);
    assert_eq!(
        reasons(&t),
        [
            "Over the 1 MB limit of a file.",
            "Over the 4 MB limit of an issue's files."
        ]
    );
}

#[test]
fn text_is_held_to_its_bytes_and_the_rest_is_never_fetched() {
    let dir = scratch();
    let (bin, pdf, svg) = (file(1, "a.md"), file(2, "a.pdf"), file(3, "a.svg"));
    let mut net = Fake::default();
    net.bytes.insert(bin.clone(), b"a\0b".to_vec());
    let mut t = take(&net, &dir, ALL);
    t.all(&[body(&bin), body(&pdf), body(&svg)]).unwrap();
    assert!(t.files.is_empty());
    let other = "Not a png, jpg, webp or gif image, nor a md, txt, csv, json, yaml, log, html, css or js file.";
    assert_eq!(
        reasons(&t),
        [
            "Not valid UTF-8 text, or it holds a NUL byte.",
            other,
            other
        ]
    );
    assert_eq!(net.fetched.borrow().len(), 1);
}

#[test]
fn a_fetch_that_fails_stops_the_start_and_says_which_link() {
    let dir = scratch();
    let link = file(1, "a.md");
    let mut net = Fake::default();
    net.rc.insert(link.clone(), 22);
    let mut t = take(&net, &dir, ALL);
    assert_eq!(
        t.all(&[body(&link)]),
        Err(format!("{link} could not be fetched"))
    );
}

// --- Archives ------------------------------------------------------------------

#[test]
fn an_archive_is_known_by_its_first_bytes_and_its_scratch_is_gone() {
    let dir = scratch();
    let (fake, good) = (file(1, "a.zip"), file(2, "b.zip"));
    let mut net = Fake::default();
    net.bytes.insert(fake.clone(), b"not a zip".to_vec());
    net.bytes.insert(
        good.clone(),
        zip_of(&[
            ("../up.md", b"x"),
            ("dir/a.md", b"ok"),
            ("b.png", PNG),
            ("c.html", PNG),
        ]),
    );
    let mut t = take(&net, &dir, ALL);
    t.all(&[body(&fake), body(&good)]).unwrap();
    assert_eq!(names(&t.images), ["image-1.png"]);
    assert_eq!(names(&t.files), ["file-1.md"]);
    assert_eq!(t.files[0].shown("name"), "a.md");
    assert_eq!(t.files[0].shown("path"), "dir/a.md");
    assert_eq!(t.files[0].shown("zip"), good);
    assert_eq!(
        t.left[0..2]
            .iter()
            .map(|e| (e.shown("reason"), e.shown("path")))
            .collect::<Vec<_>>(),
        [
            ("Not a zip archive.".to_string(), "null".to_string()),
            (
                "A path outside the archive.".to_string(),
                "../up.md".to_string()
            )
        ]
    );
    assert_eq!(listing(&dir), ["file-1.md", "image-1.png"]);
}

#[test]
fn at_most_two_archives_and_a_file_of_one_keeps_what_the_directory_holds() {
    let dir = scratch();
    let links: Vec<Linked> = (0..3).map(|i| body(&file(i, "a.zip"))).collect();
    let mut net = Fake::default();
    for l in &links {
        net.bytes.insert(l.link.clone(), zip_of(&[("a.png", PNG)]));
    }
    let mut t = take(&net, &dir, ALL);
    fs::write(format!("{dir}/image-1.png"), "kept").unwrap();
    t.all(&links).unwrap();
    assert_eq!(names(&t.images), ["2-image-1.png", "image-2.png"]);
    assert_eq!(reasons(&t), ["More than 2 archives."]);
    assert_eq!(fs::read(format!("{dir}/image-1.png")).unwrap(), b"kept");
}

#[test]
fn an_archive_pays_the_limits_of_what_it_holds() {
    let dir = scratch();
    let zip = file(1, "a.zip");
    let entries: Vec<(String, &[u8])> = (0..4).map(|i| (format!("{i}.md"), &b"x"[..])).collect();
    let refs: Vec<(&str, &[u8])> = entries.iter().map(|(n, d)| (n.as_str(), *d)).collect();
    let mut net = Fake::default();
    net.bytes.insert(zip.clone(), zip_of(&refs));
    let mut t = take(&net, &dir, ALL);
    t.n.fcount = 14;
    t.all(&[body(&zip)]).unwrap();
    assert_eq!(names(&t.files), ["file-15.md", "file-16.md"]);
    assert_eq!(reasons(&t), ["More than 2 files."; 2]);
}

#[test]
fn what_the_archive_did_is_left_out_and_what_the_host_did_stops_the_start() {
    let zip = file(1, "a.zip");
    let run = |word| {
        let dir = scratch();
        let mut net = Fake {
            unpack: Some(Failure2(word)),
            ..Fake::default()
        };
        net.bytes.insert(zip.clone(), zip_of(&[("a.md", b"x")]));
        let mut t = take(&net, &dir, ALL);
        let got = t.all(&[body(&zip)]);
        assert_eq!(listing(&dir), Vec::<String>::new());
        (got, reasons(&t))
    };
    assert_eq!(
        run("unreadable"),
        (Ok(()), vec!["Not a zip archive.".to_string()])
    );
    assert_eq!(
        run("crashed"),
        (Ok(()), vec!["Not a zip archive.".to_string()])
    );
    assert_eq!(
        run("timeout"),
        (
            Ok(()),
            vec!["An archive that took more than 60 seconds to unpack.".to_string()]
        )
    );
    for word in ["", "unwritable", "absent"] {
        let (got, left) = run(word);
        assert_eq!(
            got,
            Err(format!(
                "{zip} could not be unpacked (attach: unpack exited 1: no)"
            ))
        );
        assert!(left.is_empty());
    }
}

/// A child that answers with a path of its own: the path is not followed.
#[test]
fn a_path_out_of_a_child_is_held_to_the_shape_of_a_cleaned_name() {
    struct Lying;
    impl Net for Lying {
        fn fetch(&self, _: &str, file: &str, _: i64) -> i32 {
            fs::write(file, zip_of(&[("a.md", b"x")])).unwrap();
            0
        }
        fn attach(&self, verb: &str, _: &[String]) -> Result<Value, Failure> {
            if verb == "sniff" {
                let sha = "0".repeat(64);
                return Ok(
                    json!({"bytes": 1, "sha256": sha, "image": null, "zip": true, "text": false}),
                );
            }
            let e = |name: &str| json!({"name": name, "kind": "part", "bytes": 1, "sha256": "aa", "image": null, "text": true});
            Ok(
                json!({"entries": [e("../../secret.md"), e("/etc/passwd.md"), e("a/../b.md"), e("gone.md")]}),
            )
        }
    }
    let dir = scratch();
    let wants = ALL;
    let mut t = Take {
        net: &Lying,
        dir: &dir,
        wants,
        approved: true,
        who: "",
        allowed: "",
        n: Counts::default(),
        images: Vec::new(),
        files: Vec::new(),
        left: Vec::new(),
    };
    t.all(&[body(&file(1, "a.zip"))]).unwrap();
    assert!(t.files.is_empty());
    assert_eq!(reasons(&t), ["A path outside the archive."; 4]);
}

#[test]
fn a_name_is_listed_as_it_stood_in_the_lines_it_was_read_from() {
    let dir = scratch();
    let zip = file(1, "a.zip");
    let mut net = Fake::default();
    net.bytes
        .insert(zip.clone(), zip_of(&[("b\\c\td\ne.exe", b"x"), ("", b"x")]));
    let mut t = take(&net, &dir, ALL);
    t.all(&[body(&zip)]).unwrap();
    assert_eq!(t.left[0].shown("path"), "b\\\\c\\td\\ne.exe");
    assert_eq!(t.left[1].shown("path"), "-");
}

// --- A manifest ----------------------------------------------------------------

#[test]
fn a_manifest_is_printed_as_jq_prints_it_with_its_keys_in_order() {
    use crate::manifest::{left, of_issue};
    let long = format!("https://x/{}", "a".repeat(600));
    let lefts: Vec<Ordered> = (0..103).map(|_| left(&long, "why", "body", None)).collect();
    let m = of_issue(
        Vec::new(),
        Some(Vec::new()),
        lefts,
        json!(7),
        Ordered::Plain(Value::Null),
        Some("retry-context.md"),
    );
    let text = m.pretty();
    let v: Value = serde_json::from_str(&text).unwrap();
    assert_eq!(v["leftOut"].as_array().unwrap().len(), 100);
    assert_eq!(v["leftOutMore"], 3);
    assert_eq!(v["leftOut"][0]["link"].as_str().unwrap().len(), 512);
    // What the improvement loop copies whole: tehas-cycle's MANIFEST_BYTES.
    assert!(text.len() < 262_144);
    let keys: Vec<&str> = text
        .lines()
        .filter(|l| l.starts_with("  \""))
        .map(|l| l.trim().split('"').nth(1).unwrap())
        .collect();
    assert_eq!(
        keys,
        [
            "images",
            "files",
            "leftOut",
            "issue",
            "approval",
            "retryContext",
            "leftOutMore"
        ]
    );
    let small = of_issue(
        Vec::new(),
        None,
        Vec::new(),
        json!(7),
        Ordered::Plain(Value::Null),
        None,
    );
    assert_eq!(
        small.pretty(),
        "{\n  \"images\": [],\n  \"leftOut\": [],\n  \"issue\": 7,\n  \"approval\": null,\n  \"leftOutMore\": 0\n}\n"
    );
}

// --- The two verbs -------------------------------------------------------------

fn run_of<'a>(dir: &'a str, data: &'a str, record: &'a str) -> IssueRun<'a> {
    IssueRun {
        issue: "7",
        dir,
        repo: "acme/widget",
        approved: "approved",
        record,
        wants: ALL,
        comments: true,
        allowed: "alice",
        millwright: "",
        data,
    }
}

#[test]
fn a_directory_is_remade_and_a_bad_answer_is_no_ground_to_fetch() {
    let base = scratch();
    let dir = format!("{base}/images");
    fs::create_dir_all(&dir).unwrap();
    fs::write(format!("{dir}/old"), "x").unwrap();
    let net = Fake::default();
    for bad in ["[]", "{", r#"{"body":7}"#, "{}{}"] {
        assert_eq!(
            issue(&run_of(&dir, bad, ""), &net),
            Err("the issue could not be read for its images".to_string())
        );
    }
    assert!(net.fetched.borrow().is_empty());
    let data = json!({"author_association": "NONE", "body": file(1, "a.md")}).to_string();
    let mut run = run_of(&dir, &data, "");
    run.approved = "yes";
    issue(&run, &net).unwrap();
    assert_eq!(listing(&dir), ["manifest.json"]);
    let m: Value =
        serde_json::from_str(&fs::read_to_string(format!("{dir}/manifest.json")).unwrap()).unwrap();
    assert_eq!(m["approval"], Value::Null);
    assert_eq!(m["files"], json!([]));
    assert_eq!(m["leftOut"][0]["source"], "body");
}

#[test]
fn with_a_record_only_its_links_are_taken_and_it_is_named() {
    let base = scratch();
    let dir = format!("{base}/images");
    let record = format!("{base}/record.json");
    let link = file(1, "a.png");
    fs::write(
        &record,
        json!({
            "at": "t", "by": "alice", "how": "label", "links": [link, "https://example.com/x.png"],
            "comment_links": [
                {"link": file(2, "b.png"), "source": "comment:1", "by": "alice"},
                {"link": file(3, "c.png"), "source": "comment:2", "by": "mallory"},
                {"link": file(4, "d.png"), "source": "other:3", "by": "alice"},
            ],
        })
        .to_string(),
    )
    .unwrap();
    let data = json!({"author_association": "OWNER", "body": file(9, "body.png")}).to_string();
    let net = Fake::default();
    issue(&run_of(&dir, &data, &record), &net).unwrap();
    let m: Value =
        serde_json::from_str(&fs::read_to_string(format!("{dir}/manifest.json")).unwrap()).unwrap();
    assert_eq!(
        m["approval"],
        json!({"at": "t", "by": "alice", "how": "label"})
    );
    let fetched: Vec<String> = net
        .fetched
        .borrow()
        .iter()
        .map(|(l, _)| l.clone())
        .collect();
    assert_eq!(fetched, [link, file(2, "b.png")]);
    assert_eq!(m["leftOut"][0]["link"], file(3, "c.png"));
    assert_eq!(m["leftOut"].as_array().unwrap().len(), 1);
}

#[test]
fn a_reply_adds_to_the_session_and_a_failure_leaves_it_as_it_was() {
    let base = scratch();
    let dir = format!("{base}/images");
    fs::create_dir_all(&dir).unwrap();
    let first = file(1, "a.png");
    fs::write(format!("{dir}/image-1.png"), PNG).unwrap();
    let manifest = json!({"images": [{"file": "image-1.png", "link": first, "bytes": PNG.len(), "source": "body"}],
        "files": [], "leftOut": [], "issue": 7, "approval": null, "leftOutMore": 0}).to_string();
    fs::write(format!("{dir}/manifest.json"), &manifest).unwrap();
    let text = format!("{base}/reply");
    let (second, gone, pdf) = (file(2, "b.png"), file(3, "c.md"), file(4, "d.pdf"));
    fs::write(&text, format!("{first} {second} {pdf}")).unwrap();
    let run = AnswerRun {
        dir: &dir,
        text: &text,
        id: "42",
        wants: ALL,
    };
    let net = Fake::default();
    let (new, left) = answer(&run, &net).unwrap();
    assert_eq!(new, format!("- image-2.png, from {second}"));
    assert!(left.starts_with(&format!("- {pdf}: Not a png")));
    assert_eq!(net.fetched.borrow().len(), 1);
    assert_eq!(listing(&base), ["images", "reply"]);
    assert_eq!(
        listing(&dir),
        ["image-1.png", "image-2.png", "manifest.json"]
    );

    let before = fs::read(format!("{dir}/manifest.json")).unwrap();
    fs::write(&text, &gone).unwrap();
    let mut net = Fake::default();
    net.rc.insert(gone.clone(), 22);
    assert_eq!(
        answer(&run, &net),
        Err(format!("{gone} could not be fetched"))
    );
    assert_eq!(fs::read(format!("{dir}/manifest.json")).unwrap(), before);
    assert_eq!(listing(&base), ["images", "reply"]);

    fs::write(format!("{dir}/manifest.json"), "{}").unwrap();
    assert_eq!(
        answer(&run, &net),
        Err("the session's images could not be read".to_string())
    );
}
