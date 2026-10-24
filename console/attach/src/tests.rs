use crate::sniff::{image_type, Sniffer};
use crate::unpack::{clean_part, kind, unpack};
use crate::zip::{Archive, MAX_ENTRIES};
use flate2::write::DeflateEncoder;
use flate2::{Compression, Crc};
use serde_json::Value;
use std::fs;
use std::io::Write;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicUsize, Ordering};

// --- An archive written here, entry by entry ---------------------------------

struct E {
    name: Vec<u8>,
    data: Vec<u8>,
    // The mode in the high half of the external attributes, written as Unix.
    mode: Option<u32>,
    dos_dir: bool,
    deflate: bool,
    locked: bool,
    bad_crc: bool,
}

fn e(name: &str, data: &str) -> E {
    E {
        name: name.as_bytes().to_vec(),
        data: data.as_bytes().to_vec(),
        mode: Some(0o100644),
        dos_dir: false,
        deflate: false,
        locked: false,
        bad_crc: false,
    }
}

fn zip_of(entries: &[E]) -> Vec<u8> {
    zip_counted(entries, None)
}

// `total` overrides the count the end record says.
fn zip_counted(entries: &[E], total: Option<u16>) -> Vec<u8> {
    let mut out = Vec::new();
    let mut central = Vec::new();
    for en in entries {
        let packed = if en.deflate {
            let mut z = DeflateEncoder::new(Vec::new(), Compression::default());
            z.write_all(&en.data).unwrap();
            z.finish().unwrap()
        } else {
            en.data.clone()
        };
        let mut crc = Crc::new();
        crc.update(&en.data);
        let sum = if en.bad_crc { crc.sum() ^ 1 } else { crc.sum() };
        let method: u16 = if en.deflate { 8 } else { 0 };
        let flags: u16 = if en.locked { 1 } else { 0 };
        let offset = out.len() as u32;
        out.extend_from_slice(&0x0403_4b50u32.to_le_bytes());
        out.extend_from_slice(&20u16.to_le_bytes());
        out.extend_from_slice(&flags.to_le_bytes());
        out.extend_from_slice(&method.to_le_bytes());
        out.extend_from_slice(&[0; 4]);
        out.extend_from_slice(&sum.to_le_bytes());
        out.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        out.extend_from_slice(&(en.data.len() as u32).to_le_bytes());
        out.extend_from_slice(&(en.name.len() as u16).to_le_bytes());
        out.extend_from_slice(&0u16.to_le_bytes());
        out.extend_from_slice(&en.name);
        out.extend_from_slice(&packed);

        let made_by: u16 = if en.mode.is_some() { 3 << 8 | 20 } else { 20 };
        let external = (en.mode.unwrap_or(0) << 16) | if en.dos_dir { 0x10 } else { 0 };
        central.extend_from_slice(&0x0201_4b50u32.to_le_bytes());
        central.extend_from_slice(&made_by.to_le_bytes());
        central.extend_from_slice(&20u16.to_le_bytes());
        central.extend_from_slice(&flags.to_le_bytes());
        central.extend_from_slice(&method.to_le_bytes());
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&sum.to_le_bytes());
        central.extend_from_slice(&(packed.len() as u32).to_le_bytes());
        central.extend_from_slice(&(en.data.len() as u32).to_le_bytes());
        central.extend_from_slice(&(en.name.len() as u16).to_le_bytes());
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&[0; 4]);
        central.extend_from_slice(&external.to_le_bytes());
        central.extend_from_slice(&offset.to_le_bytes());
        central.extend_from_slice(&en.name);
    }
    let cd_offset = out.len() as u32;
    let n = total.unwrap_or(entries.len() as u16);
    out.extend_from_slice(&central);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&n.to_le_bytes());
    out.extend_from_slice(&(central.len() as u32).to_le_bytes());
    out.extend_from_slice(&cd_offset.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

// --- A scratch directory per test ----------------------------------------------

static N: AtomicUsize = AtomicUsize::new(0);

struct Scratch(PathBuf);

impl Scratch {
    fn new() -> Scratch {
        let p = std::env::temp_dir().join(format!(
            "attach-test-{}-{}",
            std::process::id(),
            N.fetch_add(1, Ordering::SeqCst)
        ));
        let _ = fs::remove_dir_all(&p);
        fs::create_dir_all(&p).unwrap();
        Scratch(p)
    }
}

impl Drop for Scratch {
    fn drop(&mut self) {
        let _ = fs::remove_dir_all(&self.0);
    }
}

fn run(zip: &[u8], dest: &Path, bytes: u64, files: i64) -> Vec<Value> {
    let archive = Archive::open(zip).expect("a readable archive");
    unpack(&archive, dest, bytes, files).expect("a directory that can be made")
}

fn skips(out: &[Value]) -> Vec<(String, String)> {
    out.iter()
        .filter(|v| v.get("skip").is_some())
        .map(|v| {
            (
                v["skip"].as_str().unwrap().to_string(),
                v["reason"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

fn names(out: &[Value]) -> Vec<String> {
    out.iter()
        .filter_map(|v| v.get("name").and_then(Value::as_str).map(String::from))
        .collect()
}

fn files_under(dir: &Path) -> Vec<String> {
    let mut found = Vec::new();
    fn walk(base: &Path, at: &Path, found: &mut Vec<String>) {
        for d in fs::read_dir(at).unwrap() {
            let p = d.unwrap().path();
            let m = fs::symlink_metadata(&p).unwrap();
            if m.is_dir() {
                walk(base, &p, found);
            } else {
                found.push(p.strip_prefix(base).unwrap().to_string_lossy().into_owned());
            }
        }
    }
    if dir.exists() {
        walk(dir, dir, &mut found);
    }
    found.sort();
    found
}

const MB: u64 = 1024 * 1024;

// --- Sniffing ------------------------------------------------------------------

fn sniffed(bytes: &[u8]) -> crate::sniff::Sniffed {
    let mut s = Sniffer::default();
    s.feed(bytes);
    s.finish()
}

#[test]
fn an_image_is_known_by_its_first_bytes() {
    assert_eq!(image_type(b"\x89PNG\r\n\x1a\nrest"), Some("png"));
    assert_eq!(image_type(b"\xff\xd8\xff\xe0"), Some("jpg"));
    assert_eq!(image_type(b"GIF87a.."), Some("gif"));
    assert_eq!(image_type(b"GIF89a.."), Some("gif"));
    assert_eq!(image_type(b"RIFF\0\0\0\0WEBPVP8 "), Some("webp"));
    assert_eq!(image_type(b"RIFF\0\0\0\0WAVE"), None);
    assert_eq!(image_type(b"\x89PNG"), None);
    assert_eq!(image_type(b"<svg"), None);
    assert_eq!(image_type(b""), None);
}

#[test]
fn a_zip_is_known_by_its_four_bytes_and_nothing_else() {
    assert!(sniffed(b"PK\x03\x04rest").zip);
    assert!(!sniffed(b"PK\x05\x06").zip);
    assert!(!sniffed(b"not a zip at all\n").zip);
    assert!(!sniffed(b"").zip);
}

#[test]
fn text_is_valid_utf8_with_no_nul_byte() {
    assert!(sniffed("café\n".as_bytes()).text);
    assert!(sniffed(b"").text);
    assert!(!sniffed(b"caf\xe9\n").text);
    assert!(!sniffed(b"a\0b").text);
    assert!(!sniffed(b"\xc3").text);
}

#[test]
fn text_cut_between_writes_inside_one_character_is_still_text() {
    let mut s = Sniffer::default();
    let bytes = "aé€😀z".as_bytes();
    for b in bytes {
        s.feed(std::slice::from_ref(b));
    }
    let got = s.finish();
    assert!(got.text);
    assert_eq!(got.bytes, bytes.len() as u64);
}

#[test]
fn the_sha256_and_the_size_are_those_of_every_byte() {
    let got = sniffed(b"abc");
    assert_eq!(got.bytes, 3);
    assert_eq!(
        got.sha256,
        "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
    );
    let mut s = Sniffer::default();
    s.feed(b"a");
    s.feed(b"bc");
    assert_eq!(s.finish().sha256, got.sha256);
}

// --- The names an archive gives ----------------------------------------------

#[test]
fn a_part_keeps_letters_digits_dot_underscore_and_dash() {
    assert_eq!(clean_part(b"a[1].txt"), "a-1-.txt");
    assert_eq!(clean_part(b"-x.txt"), "x.txt");
    assert_eq!(clean_part(b"..hidden"), "hidden");
    assert_eq!(clean_part(b"a b.txt"), "a-b.txt");
    assert_eq!(clean_part(b"..."), "");
    // Every byte of a name that is not UTF-8, or not ASCII, is one dash.
    assert_eq!(clean_part(b"x\xff\xfe.txt"), "x--.txt");
    assert_eq!(clean_part("é.txt".as_bytes()), "txt");
    assert_eq!(clean_part(&[b'a'; 100]).len(), 80);
}

#[test]
fn the_kinds_are_the_labs_and_md() {
    for (ext, want) in [
        ("png", "image"),
        ("svg", "image"),
        ("mp4", "video"),
        ("htm", "app"),
        ("mjs", "part"),
        ("md", "part"),
    ] {
        assert_eq!(kind(ext), Some(want), "{ext}");
    }
    for ext in ["zip", "pdf", "csv", "yaml", ""] {
        assert_eq!(kind(ext), None, "{ext}");
    }
}

// --- Unpacking -------------------------------------------------------------------

#[test]
fn files_are_written_with_what_their_bytes_say() {
    let s = Scratch::new();
    let mut png = e("img/b.png", "");
    png.data = b"\x89PNG\r\n\x1a\nDATA".to_vec();
    png.deflate = true;
    let zip = zip_of(&[e("a.md", "# A\n"), png]);
    let out = run(&zip, &s.0, MB, 16);
    assert_eq!(names(&out), ["a.md", "img/b.png"]);
    assert_eq!(out[0]["kind"], "part");
    assert_eq!(out[0]["bytes"], 4);
    assert_eq!(out[0]["text"], true);
    assert_eq!(out[0]["image"], Value::Null);
    assert_eq!(out[1]["kind"], "image");
    assert_eq!(out[1]["image"], "png");
    assert_eq!(out[1]["text"], false);
    assert_eq!(fs::read(s.0.join("a.md")).unwrap(), b"# A\n");
    let mode = fs::metadata(s.0.join("img/b.png"))
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o644);
    assert_eq!(out[0]["sha256"].as_str().unwrap().len(), 64);
}

#[test]
fn a_path_out_of_the_directory_writes_nothing() {
    let s = Scratch::new();
    let dest = s.0.join("d");
    let zip = zip_of(&[
        e("../escape.txt", "x"),
        e("/abs.txt", "x"),
        e("a/../../up.txt", "x"),
        e("back\\slash.txt", "x"),
        e("1/2/3/4/5/6/deep.txt", "x"),
        e("1/2/3/4/5/ok.txt", "ok"),
    ]);
    let out = run(&zip, &dest, MB, 16);
    assert_eq!(names(&out), ["1/2/3/4/5/ok.txt"]);
    let why: Vec<_> = skips(&out).into_iter().map(|(_, r)| r).collect();
    assert_eq!(why, vec!["A path outside the archive."; 5]);
    assert_eq!(files_under(&s.0), ["d/1/2/3/4/5/ok.txt"]);
}

#[test]
fn a_link_or_a_special_file_is_left_out_and_never_made() {
    let s = Scratch::new();
    let mut link = e("link", "/etc/passwd");
    link.mode = Some(0o120777);
    let mut fifo = e("fifo.txt", "");
    fifo.mode = Some(0o010644);
    let mut dir = e("folder/", "");
    dir.mode = Some(0o040755);
    let zip = zip_of(&[link, fifo, dir, e("ok.txt", "ok")]);
    let out = run(&zip, &s.0, MB, 16);
    assert_eq!(
        skips(&out),
        [
            ("link".to_string(), "A link or a special file.".to_string()),
            (
                "fifo.txt".to_string(),
                "A link or a special file.".to_string()
            ),
        ]
    );
    assert_eq!(names(&out), ["ok.txt"]);
    assert_eq!(files_under(&s.0), ["ok.txt"]);
}

#[test]
fn an_entry_with_no_unix_type_is_a_file_and_a_folder_by_its_name_is_passed_over() {
    let s = Scratch::new();
    let mut plain = e("plain.txt", "plain");
    plain.mode = Some(0o644);
    let mut dos = e("dos.txt", "dos");
    dos.mode = None;
    let mut folder = e("sub/", "");
    folder.mode = None;
    let mut dosdir = e("other", "");
    dosdir.mode = None;
    dosdir.dos_dir = true;
    let out = run(&zip_of(&[plain, dos, folder, dosdir]), &s.0, MB, 16);
    assert_eq!(names(&out), ["plain.txt", "dos.txt"]);
    assert!(skips(&out).is_empty());
}

#[test]
fn a_locked_or_damaged_file_is_left_out_and_nothing_of_it_kept() {
    let s = Scratch::new();
    let mut locked = e("locked.txt", "LOCKED");
    locked.locked = true;
    let mut bad = e("bad.txt", "BAD");
    bad.bad_crc = true;
    let mut badz = e("badz.txt", "BAD BUT DEFLATED");
    badz.deflate = true;
    badz.bad_crc = true;
    let out = run(&zip_of(&[locked, bad, badz]), &s.0, MB, 16);
    assert_eq!(
        skips(&out),
        [
            ("locked.txt".to_string(), "A locked file.".to_string()),
            ("bad.txt".to_string(), "Could not be unpacked.".to_string()),
            ("badz.txt".to_string(), "Could not be unpacked.".to_string()),
        ]
    );
    assert!(files_under(&s.0).is_empty());
}

#[test]
fn names_that_clean_to_one_keep_the_first() {
    let s = Scratch::new();
    let out = run(
        &zip_of(&[
            e("a b.txt", "first"),
            e("a-b.txt", "second"),
            e("a b.txt", "third"),
        ]),
        &s.0,
        MB,
        16,
    );
    assert_eq!(names(&out), ["a-b.txt"]);
    assert_eq!(
        skips(&out),
        [
            (
                "a-b.txt".to_string(),
                "A second file of that name.".to_string()
            ),
            (
                "a b.txt".to_string(),
                "A second file of that name.".to_string()
            ),
        ]
    );
    assert_eq!(fs::read_to_string(s.0.join("a-b.txt")).unwrap(), "first");
}

#[test]
fn a_file_where_a_folder_is_needed_leaves_the_second_out() {
    let s = Scratch::new();
    let out = run(
        &zip_of(&[e("d.txt", "d"), e("d.txt/inner.txt", "inner")]),
        &s.0,
        MB,
        16,
    );
    assert_eq!(names(&out), ["d.txt"]);
    assert_eq!(
        skips(&out),
        [(
            "d.txt/inner.txt".to_string(),
            "A name the lab cannot use.".to_string()
        )]
    );
}

#[test]
fn a_name_that_cleans_to_nothing_or_has_an_empty_part_is_left_out() {
    let s = Scratch::new();
    let out = run(
        &zip_of(&[e("./a.txt", "x"), e("a//b.txt", "x"), e("...", "x")]),
        &s.0,
        MB,
        16,
    );
    assert!(names(&out).is_empty());
    assert_eq!(skips(&out).len(), 3);
    assert!(skips(&out)
        .iter()
        .all(|(_, r)| r == "A name the lab cannot use."));
}

#[test]
fn a_name_that_is_not_utf8_is_cleaned_and_said_with_replacement_characters() {
    let s = Scratch::new();
    let mut odd = e("", "x");
    odd.name = b"caf\xe9.txt".to_vec();
    let mut nul = e("", "y");
    nul.name = b"n\0l.txt".to_vec();
    let mut evil = e("", "z");
    evil.name = b"\xff/../x.txt".to_vec();
    let out = run(&zip_of(&[odd, nul, evil]), &s.0, MB, 16);
    assert_eq!(names(&out), ["caf-.txt", "n-l.txt"]);
    assert_eq!(
        skips(&out),
        [(
            "\u{fffd}/../x.txt".to_string(),
            "A path outside the archive.".to_string()
        )]
    );
}

#[test]
fn only_the_types_the_lab_hosts_and_md() {
    let s = Scratch::new();
    let out = run(
        &zip_of(&[e("notes.pdf", "%PDF"), e("x.CSS", "a{}"), e("noext", "x")]),
        &s.0,
        MB,
        16,
    );
    assert_eq!(names(&out), ["x.CSS"]);
    assert_eq!(
        skips(&out),
        [
            (
                "notes.pdf".to_string(),
                "Not a type the lab hosts.".to_string()
            ),
            ("noext".to_string(), "Not a type the lab hosts.".to_string()),
        ]
    );
}

#[test]
fn at_most_the_files_given() {
    let s = Scratch::new();
    let entries: Vec<E> = (1..=5).map(|i| e(&format!("f{i}.txt"), "x")).collect();
    let out = run(&zip_of(&entries), &s.0, MB, 3);
    assert_eq!(names(&out), ["f1.txt", "f2.txt", "f3.txt"]);
    assert_eq!(
        skips(&out),
        [
            ("f4.txt".to_string(), "More than 3 files.".to_string()),
            ("f5.txt".to_string(), "More than 3 files.".to_string()),
        ]
    );
}

#[test]
fn a_bomb_is_held_to_the_bytes_written_not_to_what_it_claims() {
    let s = Scratch::new();
    let mut bomb = e("big.txt", "");
    bomb.data = vec![0u8; 11 * MB as usize];
    bomb.deflate = true;
    let zip = zip_of(&[bomb, e("after.txt", "x")]);
    assert!(zip.len() < 64 * 1024);
    let out = run(&zip, &s.0, 10 * MB, 40);
    assert!(names(&out).is_empty());
    assert_eq!(
        skips(&out),
        [
            (
                "big.txt".to_string(),
                "Over the 10 MB limit of a ticket.".to_string()
            ),
            (
                "after.txt".to_string(),
                "Over the 10 MB limit of a ticket.".to_string()
            ),
        ]
    );
    assert!(files_under(&s.0).is_empty());
}

#[test]
fn small_files_together_are_held_to_what_is_left() {
    let s = Scratch::new();
    let entries: Vec<E> = ["a", "b", "c"]
        .iter()
        .map(|n| {
            let mut x = e(&format!("{n}.txt"), "");
            x.data = vec![b'z'; 4];
            x
        })
        .collect();
    let out = run(&zip_of(&entries), &s.0, 9, 16);
    assert_eq!(names(&out), ["a.txt", "b.txt"]);
    assert_eq!(
        skips(&out),
        [(
            "c.txt".to_string(),
            "Over the 10 MB limit of a ticket.".to_string()
        )]
    );
    // Exactly the room left is taken.
    let s2 = Scratch::new();
    assert_eq!(
        names(&run(&zip_of(&entries), &s2.0, 8, 16)),
        ["a.txt", "b.txt"]
    );
}

#[test]
fn a_size_the_archive_lies_about_is_damage() {
    let s = Scratch::new();
    let mut zip = zip_of(&[e("a.txt", "abcd")]);
    // The central record's uncompressed size, 4, said as 2.
    let cd = zip.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    zip[cd + 24] = 2;
    let out = run(&zip, &s.0, MB, 16);
    assert_eq!(
        skips(&out),
        [("a.txt".to_string(), "Could not be unpacked.".to_string())]
    );
}

#[test]
fn only_the_first_200_entries_are_read() {
    let s = Scratch::new();
    let entries: Vec<E> = (1..=205).map(|i| e(&format!("e{i}.txt"), "x")).collect();
    let out = run(&zip_of(&entries), &s.0, MB, 1000);
    assert_eq!(names(&out).len(), MAX_ENTRIES);
    assert_eq!(out.last().unwrap()["skip"], "...");
    assert_eq!(
        out.last().unwrap()["reason"],
        "More than 200 entries in the zip."
    );
    assert!(!s.0.join("e201.txt").exists());
}

#[test]
fn nothing_is_written_through_a_link() {
    let s = Scratch::new();
    let dest = s.0.join("unz");
    let outside = s.0.join("outside");
    fs::create_dir_all(&dest).unwrap();
    fs::create_dir_all(&outside).unwrap();
    symlink(&outside, dest.join("img")).unwrap();
    symlink(outside.join("target.md"), dest.join("dangling.md")).unwrap();
    let zip = zip_of(&[e("img/b.png", "x"), e("dangling.md", "x"), e("a.md", "a")]);
    let out = run(&zip, &dest, MB, 16);
    assert_eq!(names(&out), ["a.md"]);
    assert_eq!(
        skips(&out),
        [
            (
                "img/b.png".to_string(),
                "A path outside the archive.".to_string()
            ),
            (
                "dangling.md".to_string(),
                "A path outside the archive.".to_string()
            ),
        ]
    );
    assert!(files_under(&outside).is_empty());

    // The directory given is a link: nothing is written at all.
    let linked = s.0.join("linked");
    symlink(&outside, &linked).unwrap();
    let out = run(&zip, &linked, MB, 16);
    assert!(names(&out).is_empty());
    assert!(files_under(&outside).is_empty());
}

#[test]
fn an_archive_that_cannot_be_read_is_refused_whole() {
    assert!(Archive::open(b"").is_err());
    assert!(Archive::open(b"not a zip at all, but long enough to search").is_err());
    let good = zip_of(&[e("a.txt", "a")]);
    assert!(Archive::open(&good).is_ok());
    // Cut short, the end record is gone.
    assert!(Archive::open(&good[..good.len() - 5]).is_err());
    // A count the directory does not hold.
    assert!(Archive::open(&zip_counted(&[e("a.txt", "a")], Some(2))).is_err());
    // A directory said to be bigger than the archive.
    let mut big = good.clone();
    let at = big.len() - 22 + 12;
    big[at..at + 4].copy_from_slice(&0x00ff_ffffu32.to_le_bytes());
    assert!(Archive::open(&big).is_err());
    // zip64 is not read.
    let mut z64 = good.clone();
    let at = z64.len() - 22 + 16;
    z64[at..at + 4].copy_from_slice(&0xffff_ffffu32.to_le_bytes());
    assert!(Archive::open(&z64).is_err());
}

#[test]
fn every_cut_and_every_flipped_byte_of_an_archive_is_read_without_a_panic() {
    let mut png = e("p.png", "");
    png.data = b"\x89PNG\r\n\x1a\n0123456789".to_vec();
    png.deflate = true;
    let good = zip_of(&[e("a.md", "# a"), png, e("dir/b.txt", "bbbb")]);
    let s = Scratch::new();
    for cut in 0..good.len() {
        if let Ok(a) = Archive::open(&good[..cut]) {
            let d = s.0.join(format!("c{cut}"));
            let _ = unpack(&a, &d, MB, 16);
        }
    }
    for at in 0..good.len() {
        for flip in [0x01u8, 0x80, 0xff] {
            let mut z = good.clone();
            z[at] ^= flip;
            if let Ok(a) = Archive::open(&z) {
                let d = s.0.join(format!("f{at}-{flip}"));
                let _ = unpack(&a, &d, MB, 16);
            }
        }
    }
}

// One deflate stream of 10 MiB of zeros with a wrong checksum, named by `n`
// central records that all point at the one local record.
fn shared_bomb(n: usize) -> Vec<u8> {
    let mut bomb = e("a000.txt", "");
    bomb.data = vec![0u8; 10 * MB as usize];
    bomb.deflate = true;
    bomb.bad_crc = true;
    let one = zip_of(&[bomb]);
    let cd = one.windows(4).rposition(|w| w == b"PK\x01\x02").unwrap();
    let eocd = one.len() - 22;
    let record = &one[cd..eocd];
    let mut out = one[..cd].to_vec();
    let mut dir = Vec::new();
    for i in 0..n {
        let mut r = record.to_vec();
        let name = format!("a{i:03}.txt");
        r[46..46 + 8].copy_from_slice(name.as_bytes());
        dir.extend_from_slice(&r);
    }
    let at = out.len() as u32;
    out.extend_from_slice(&dir);
    out.extend_from_slice(&0x0605_4b50u32.to_le_bytes());
    out.extend_from_slice(&[0; 4]);
    out.extend_from_slice(&(n as u16).to_le_bytes());
    out.extend_from_slice(&(n as u16).to_le_bytes());
    out.extend_from_slice(&(dir.len() as u32).to_le_bytes());
    out.extend_from_slice(&at.to_le_bytes());
    out.extend_from_slice(&0u16.to_le_bytes());
    out
}

#[test]
fn entries_that_share_one_stream_are_damaged_and_never_inflated() {
    let zip = shared_bomb(200);
    assert!(zip.len() < 64 * 1024);
    let archive = Archive::open(&zip).unwrap();
    let s = Scratch::new();
    let out = unpack(&archive, &s.0, 10 * MB, 50).unwrap();
    assert!(names(&out).is_empty());
    assert_eq!(skips(&out).len(), 200);
    assert!(skips(&out)
        .iter()
        .all(|(_, r)| r == "Could not be unpacked."));
    let mut sink = Vec::new();
    let mut inflated = 0;
    for entry in &archive.entries {
        assert!(archive
            .read(entry, 10 * MB, &mut sink, &mut inflated)
            .is_err());
    }
    assert_eq!(inflated, 0);
}

#[test]
fn damaged_entries_are_charged_what_they_inflated() {
    // Two streams of their own, each 10 MiB with a wrong checksum: the first
    // spends the room and the second is not inflated past it.
    let mut a = e("a.txt", "");
    a.data = vec![0u8; 10 * MB as usize];
    a.deflate = true;
    a.bad_crc = true;
    let mut b = e("b.txt", "");
    b.data = a.data.clone();
    b.deflate = true;
    b.bad_crc = true;
    let s = Scratch::new();
    let out = run(&zip_of(&[a, b, e("c.txt", "c")]), &s.0, 10 * MB, 50);
    assert_eq!(
        skips(&out),
        [
            ("a.txt".to_string(), "Could not be unpacked.".to_string()),
            (
                "b.txt".to_string(),
                "Over the 10 MB limit of a ticket.".to_string()
            ),
            (
                "c.txt".to_string(),
                "Over the 10 MB limit of a ticket.".to_string()
            ),
        ]
    );
}

#[test]
fn an_entry_that_inflates_past_its_size_stops_there() {
    let mut big = e("big.txt", "");
    big.data = vec![0u8; MB as usize];
    big.deflate = true;
    let mut zip = zip_of(&[big]);
    // The central record says 10 bytes.
    let cd = zip.windows(4).position(|w| w == b"PK\x01\x02").unwrap();
    zip[cd + 24..cd + 28].copy_from_slice(&10u32.to_le_bytes());
    let archive = Archive::open(&zip).unwrap();
    let mut sink = Vec::new();
    let mut inflated = 0;
    assert!(archive
        .read(&archive.entries[0], 10 * MB, &mut sink, &mut inflated)
        .is_err());
    assert!(inflated <= 64 * 1024, "inflated {inflated}");
}
