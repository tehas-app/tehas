//! One pass of `tehasbot protolab`: which tickets are at the stage, what each
//! one holds, and the order of the work. Settings come from the environment
//! `console run` loaded.

use super::{front_page, live_file, ticket_page};
use crate::edge;
use crate::forge::{gh_program, repo, CALL_BOUND};
use crate::jq::J;
use serde_json::Value;
use std::env;
use std::fs;
use std::io::{self, Write};
use std::path::{Path, PathBuf};
use tehas_core::forge::{Bounded, GhRunner, GitHub};
use tehas_core::home::Home;
use tehas_core::issue_feed::{self as feed, Standing};
use tehas_core::kinds::Kind;
use tehas_items::page::number_of;
use tehas_items::store::{self, generator, raw, Store, LIMIT};

const LABEL: &str = Kind::Prototype.label();
/// What the label puts a ticket at, as a refusal says it.
const STAGE: &str = "the prototype stage";
const NO_TOKEN: &str = "no GitHub credential: GH_TOKEN is empty and gh holds none; \
a file GitHub hands out only to a signed-in account is left out";
/// The name of a feature this pass checks.
const ZIP_URL: &str = "protolab-zip-url";

/// One line of what a pass did. A log that went away does not stop the pass.
fn say(line: &str) {
    let _ = writeln!(io::stdout(), "{line}");
}

/// The settings of a pass.
struct Lab {
    site: PathBuf,
    repo: String,
    allowed: String,
    domain: String,
    url: String,
    design: String,
    /// Whether a ticket may name a zip by address: protolab-zip-url is on.
    zips: bool,
    /// Where the files are taken: TEHAS_PROTOLAB_ZIP_HOSTS are the hosts a
    /// zip by address may come from.
    store: Store,
}

pub fn run() -> u8 {
    let setting = |name: &str| env::var(name).unwrap_or_default();
    let (home, allowed) = (setting("TEHAS_HOME"), setting("TEHAS_ALLOWED_LABELERS"));
    let Some(repo) = repo().filter(|_| !home.is_empty() && !allowed.is_empty()) else {
        eprintln!(
            "tehasbot protolab: TEHAS_HOME, TEHAS_REPO and TEHAS_ALLOWED_LABELERS must be set"
        );
        return 78;
    };
    let home = PathBuf::from(home);
    let site = home.join("items");
    let live = Home::new(&home).live_dir();
    if let Err(e) = fs::create_dir_all(&site).and_then(|()| fs::create_dir_all(&live)) {
        eprintln!("tehasbot protolab: cannot make its directories: {e}");
        return 1;
    }
    let Some(_lock) = edge::lock(&home, "protolab", None) else {
        say("another tehasbot protolab is running");
        return 0;
    };
    let work = site.join(".work");
    if let Err(e) = edge::remove(&work).and_then(|()| fs::create_dir_all(&work)) {
        eprintln!("tehasbot protolab: cannot make {}: {e}", work.display());
        return 1;
    }
    let mut lab = Lab {
        site,
        repo,
        allowed,
        // What follows a ticket's label: .<domain>, or -<domain> one level
        // deep (TEHAS_EDGE_ONE_LEVEL); empty with no domain.
        domain: tehas_core::settings::edge_suffix(
            &setting("TEHAS_DOMAIN"),
            setting("TEHAS_EDGE_ONE_LEVEL") == "1",
        ),
        url: setting("TEHAS_PROTOLAB_URL"),
        design: setting("TEHAS_DESIGN"),
        zips: tehas_core::features::on(ZIP_URL),
        store: Store {
            home,
            work,
            token: String::new(),
            zip_hosts: setting("TEHAS_PROTOLAB_ZIP_HOSTS"),
        },
    };
    let rc = pass(&mut lab, &live);
    let _ = edge::remove(&lab.store.work);
    rc
}

fn pass(lab: &mut Lab, live: &Path) -> u8 {
    let program = gh_program(
        lab.store.home.to_str(),
        &env::var("PATH").unwrap_or_default(),
    )
    .unwrap_or_else(|| "gh".to_string());
    let mut gh = Bounded::new(program.clone(), CALL_BOUND);
    lab.store.token = token(&mut gh);
    if lab.store.token.is_empty() {
        say(NO_TOKEN);
    }
    let home_url = home_url(&lab.domain, &lab.url);
    let zips = lab.zips.then_some(lab.store.zip_hosts.as_str());
    let generator = generator(&lab.domain, &lab.design, zips);

    // The bot's own login, asked once before anything of a ticket is read,
    // built or removed: whatever a cycle labels, it labels as the bot. A login
    // that cannot be read ends the pass too, because no label below could then
    // be checked against the allowed list.
    let bot = env::var("TEHAS_BOT_LOGIN").unwrap_or_default();
    if bot.is_empty() {
        say("the bot's own login could not be read, nothing built or removed");
        return 0;
    }
    if let Some(line) = feed::bot_refusal(&bot, &lab.allowed) {
        say(&line);
        return 78;
    }

    let mut forge = GitHub::new(
        lab.repo.clone(),
        Box::new(Bounded::new(program, CALL_BOUND)),
    );
    // With TEHAS_ISSUE_TYPES mapping the kind, the issue type marks a ticket
    // and the label marks nothing. A type the owner does not have stops the
    // pass before anything is built or removed.
    let kind = tehas_core::kinds::type_from_env(Kind::Prototype);
    if let Some(t) = &kind {
        if let Some((rc, line)) = feed::type_fault(&mut forge, t, "prototype", &lab.repo) {
            say(&line);
            return rc;
        }
    }

    // The first hundred; a failed or odd answer leaves everything as it is.
    let filter = match &kind {
        Some(t) => format!("type={t}"),
        None => format!("labels={LABEL}"),
    };
    let list = format!("repos/{}/issues?state=open&{filter}&per_page=100", lab.repo);
    let tickets = match gh.run(&["api", &list]) {
        Ok(answer) => feed::tickets(&answer).ok_or_else(String::new),
        Err(why) => Err(why),
    };
    let tickets = match tickets {
        Ok(tickets) => tickets,
        Err(why) => {
            say(&feed::unlisted(&why));
            return 0;
        }
    };

    // With no pin, or another label under the name, no ticket is at the stage.
    let refused = match &kind {
        Some(_) => None,
        None if tickets.is_empty() => None,
        None => match feed::pin_refusal(&mut gh, &lab.store.home, &lab.repo, LABEL, STAGE) {
            Ok(refused) => refused,
            Err(line) => {
                say(&line);
                return 0;
            }
        },
    };
    if let Some(line) = &refused {
        say(line);
    }

    let mut keep = Vec::new();
    for ticket in tickets.iter().filter(|_| refused.is_none()) {
        let n = ticket["number"].as_u64().unwrap_or_default();
        let approved = match feed::standing(&mut forge, n, LABEL, kind.as_deref(), &lab.allowed) {
            Standing::Allowed(approved) => approved,
            Standing::Unread(line) => {
                // Not a refusal: nothing is built, and the removal keeps it.
                say(&line);
                keep.push(n.to_string());
                continue;
            }
            Standing::TakenOff(line) | Standing::Denied(line) => {
                say(&line);
                continue;
            }
        };
        keep.push(n.to_string());
        let stamp = format!("{generator} {}", raw(&ticket["updatedAt"]));
        let place = lab.site.join(n.to_string());
        let had = fs::read_to_string(place.join(".stamp")).unwrap_or_default();
        let mut had = had.lines();
        let (had_stamp, had_key) = (had.next(), had.next());
        if had_stamp == Some(stamp.as_str()) {
            continue;
        }
        // The page stays as it was until the label or the type is given again.
        if let Some(line) = feed::title_hold(&mut forge, n, approved, &lab.allowed) {
            say(&line);
            continue;
        }
        let allowed = |w: &str| feed::allowed(w, &lab.allowed);
        let Some(found) = feed::texts(&lab.repo, &mut gh, n)
            .map(|t| feed::refs(&t, &allowed, lab.zips, &attach::link::links))
        else {
            say(&format!("#{n}: could not be read, left as it was"));
            continue;
        };
        // A change that brings no new link and no new title does not fetch
        // the files again: the page only takes the new time.
        let key = found.key(&format!("{generator} {}", raw(&ticket["title"])));
        if had_key == Some(key.as_str())
            && retime(&place, ticket, &stamp, &key, &home_url, &lab.design).is_some()
        {
            continue;
        }
        let page = |row: &Value, files: &[Value], skipped: &[Value], outside: i64| {
            ticket_page(row, files, skipped, outside, &home_url, &lab.design, LIMIT)
        };
        let built = store::build(
            &lab.store,
            n,
            ticket,
            &found,
            &format!("{stamp}\n{key}"),
            &page,
        )
        .and_then(|(dir, line)| edge::place(&dir, &place).ok().map(|()| line));
        match built {
            Some(line) => say(&line),
            None => say(&format!("#{n}: could not be read, left as it was")),
        }
    }

    for n in edge::numbers(&lab.site) {
        if !keep.contains(&n) && edge::remove(&lab.site.join(&n)).is_ok() {
            say(&format!("#{n}: no longer at the prototype stage, removed"));
        }
    }

    let metas = metas(&lab.site);
    let now = tehas_core::time::iso(tehas_core::time::now());
    let front = front_page(&metas, &lab.domain, &home_url, &lab.design, LIMIT);
    let list = live_file(&metas, &now, &lab.url, &lab.domain, LIMIT);
    let written = edge::put(&lab.site.join("index.html"), front.as_bytes())
        .and_then(|()| edge::put(&live.join("protolab.json"), list.as_bytes()));
    if let Err(e) = written {
        eprintln!("tehasbot protolab: cannot write the list: {e}");
        return 1;
    }
    0
}

/// GH_TOKEN, or what gh holds. Fetching takes it on standard input.
fn token(gh: &mut Bounded) -> String {
    match env::var("GH_TOKEN") {
        Ok(token) if !token.is_empty() => token,
        _ => gh
            .run(&["auth", "token"])
            .map(|out| out.trim_end_matches('\n').to_string())
            .unwrap_or_default(),
    }
}

/// With a domain every ticket has a name of its own and the list is elsewhere.
fn home_url(domain: &str, url: &str) -> String {
    if domain.is_empty() {
        "/".to_string()
    } else {
        format!("{url}/")
    }
}

/// Every ticket's meta, newest first. One that cannot be read is left out, so
/// it does not cost the list its other rows.
fn metas(site: &Path) -> Vec<Value> {
    let mut names: Vec<String> = fs::read_dir(site)
        .into_iter()
        .flatten()
        .flatten()
        .filter_map(|entry| entry.file_name().into_string().ok())
        .filter(|name| !name.starts_with('.'))
        .collect();
    names.sort();
    let mut found: Vec<Value> = names
        .iter()
        .filter_map(|name| {
            let text = fs::read_to_string(site.join(name).join(".meta.json")).ok()?;
            let mut row: Value = serde_json::from_str(&text).ok()?;
            // When the page was built: a ticket left alone keeps its page.
            let built = fs::metadata(site.join(name).join("index.html"))
                .and_then(|m| m.modified())
                .ok()
                .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok());
            if let (Some(t), Some(o)) = (built, row.as_object_mut()) {
                o.insert(
                    "builtAt".into(),
                    Value::from(tehas_core::time::iso(t.as_secs() as i64)),
                );
            }
            Some(row)
        })
        .collect();
    found.sort_by_key(|row| std::cmp::Reverse(number_of(row)));
    found
}

/// The page of a ticket whose title and links are as they were, given the
/// ticket's new time in place, with no file fetched. None when the page there
/// cannot be read or written; the ticket is then built again.
fn retime(
    place: &Path,
    ticket: &Value,
    stamp: &str,
    key: &str,
    home_url: &str,
    design: &str,
) -> Option<()> {
    let mut meta = J::parse(&fs::read_to_string(place.join(".meta.json")).ok()?)?;
    let J::Obj(fields) = &mut meta else {
        return None;
    };
    let time = J::parse(&ticket["updatedAt"].to_string())?;
    fields.iter_mut().find(|(k, _)| k == "updatedAt")?.1 = time;
    let meta = meta.pretty();
    let row: Value = serde_json::from_str(&meta).ok()?;
    let outside = row["outside"].as_i64().unwrap_or(0);
    let page = ticket_page(
        &row,
        row["files"].as_array()?,
        row["skipped"].as_array()?,
        outside,
        home_url,
        design,
        LIMIT,
    );
    edge::put(&place.join(".meta.json"), format!("{meta}\n").as_bytes()).ok()?;
    edge::put(&place.join("index.html"), page.as_bytes()).ok()?;
    edge::put(
        &place.join(".stamp"),
        format!("{stamp}\n{key}\n").as_bytes(),
    )
    .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn scratch(name: &str) -> tehas_core::testdir::TestDir {
        tehas_core::testdir::TestDir::new(&format!("protolab-{name}"))
    }

    #[test]
    fn a_broken_meta_costs_the_list_only_its_own_row() {
        let site = scratch("metas");
        for (name, text) in [
            ("3", "{\"number\":3}"),
            ("11", "{\"number\":11}"),
            ("7", "not json"),
            (".old", "{\"number\":99}"),
        ] {
            fs::create_dir_all(site.join(name)).unwrap();
            fs::write(site.join(name).join(".meta.json"), text).unwrap();
        }
        let numbers: Vec<i64> = metas(&site).iter().map(number_of).collect();
        assert_eq!(numbers, [11, 3]);
    }
}
