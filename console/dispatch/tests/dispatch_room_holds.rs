//! The free disk that holds a start, and the model endpoint and its backup,
//! as the poll reads them from df, docker, models.json and curl. The pure
//! rules are in tests/fixtures/room.json.
//!
//! The model endpoint's hold sits above the disk's, so a case about the disk
//! brings its own answer for it.

mod common;

use common::{has, jq, lacks, World, GIB, MINIMAL};
use serde_json::json;
use std::fs;

/// The endpoint of the host a case builds, offering `answer`.
fn asked(w: &World, answer: &str) {
    w.mesh_curl();
    w.mesh(Some(answer));
}

const OFFERS: &str = r#"{"data":[{"id":"test-model"}]}"#;

fn full_disk(free: &str) -> String {
    format!("holding: {free} GiB free on the disk, under TEHAS_CYCLE_MIN_FREE=10g: no cycle starts")
}

const UNREAD: &str = "holding: the free disk space could not be read: no cycle starts";

/// Lines of `out` that hold `text`, as grep -c counts them.
fn count(out: &str, text: &str) -> usize {
    out.lines().filter(|l| l.contains(text)).count()
}

fn status_calls(w: &World) -> usize {
    w.calls("fake-status").lines().count()
}

fn state_text(w: &World) -> String {
    w.read_home("live/dispatch.json")
}

#[test]
fn the_lesser_of_the_two_file_systems_decides() {
    let w = World::new("room-holds");
    asked(&w, OFFERS);
    for (home, docker) in [(40, 1), (1, 40)] {
        w.reset();
        w.slots(2, &[]);
        w.ready(&[7, 8]);
        w.df_free_of(home, docker);
        let r = w.poll();
        assert_eq!(w.started(), "");
        has(&r.out, &full_disk("1.0"));
    }
}

#[test]
fn a_disk_whose_free_space_cannot_be_read_holds_no_df_no_docker_info_and_no_number() {
    let w = World::new("room-holds");
    asked(&w, OFFERS);
    let fresh = || {
        w.reset();
        w.slots(2, &[]);
        w.ready(&[7]);
    };
    fresh();
    w.touch("df-fails");
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue edit");
    has(&r.out, UNREAD);
    assert_eq!(w.dispatch_state()["why"], json!("disk"));

    // No figure at all: neither file system answers.
    fresh();
    w.df_reset();
    w.touch("df-fails");
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, UNREAD);

    // The file system Docker keeps its images on cannot be read: the figure
    // of ~/tehas is no answer for it.
    fresh();
    w.df_free_of(40, 1);
    w.fx("df-fails-of", "/srv/docker\n");
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, UNREAD);

    fresh();
    w.df_reset();
    w.fx("free", "lots\n");
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, UNREAD);
    let r = w.dispatch(&["--dry-run"]);
    has(&r.out, UNREAD);

    // Docker is not there to name the file system of its images, so df is
    // pointed at the default, and that one cannot be read.
    fresh();
    w.df_free(40);
    w.fx("df-fails-of", "/var/lib/docker\n");
    w.touch("docker-down");
    let r = w.poll();
    assert_eq!(w.started(), "");
    has(&r.out, UNREAD);
}

#[test]
fn a_real_poll_logs_a_hold_once_then_released_once_and_writes_live_dispatch_json() {
    let w = World::new("room-holds");
    asked(&w, OFFERS);
    w.reset();
    w.fx("ready.json", "[]");
    w.other(19 * GIB + GIB / 2);
    w.dispatch(&["--dry-run"]);
    assert!(!w.dispatch_json().exists());
    let r = w.poll();
    assert_eq!(
        count(&r.out, "holding: tehas.slice has no room"),
        1,
        "{}",
        r.out
    );
    // The status is written once more when a hold begins or ends.
    assert_eq!(status_calls(&w), 2);
    assert_eq!(
        jq(
            "[.why, .count, .room, .committed, .max, .reserve]",
            &state_text(&w)
        ),
        format!("[\"room\",0,0,{},{},\"2g\"]", 19 * GIB + GIB / 2, 20 * GIB)
    );
    assert_eq!(
        w.dispatch_state()["hold"],
        json!("holding: tehas.slice has no room for another cycle (19.5 GiB of 20.0 GiB promised, each cycle is given 2g)")
    );
    assert_eq!(
        jq(".time | fromdateiso8601 | (now - .) < 60", &state_text(&w)),
        "true"
    );
    let live = w.files(&w.home.join("live"));
    assert!(
        !live.iter().any(|f| f.starts_with("dispatch.json.")),
        "{live:?}"
    );
    fs::remove_file(w.t().join("calls/fake-status")).unwrap();
    let r = w.poll();
    lacks(&r.out, "holding:");
    assert_eq!(status_calls(&w), 1);
    w.other(GIB);
    let r = w.poll();
    assert_eq!(count(&r.out, "released: cycles may start again"), 1);
    assert_eq!(status_calls(&w), 3);
    lacks(&r.out, "holding:");
    assert_eq!(
        jq("[.why, .hold, .room, .committed]", &state_text(&w)),
        format!("[\"\",\"\",3,{}]", 2 * GIB)
    );
    let r = w.poll();
    lacks(&r.out, "released");
}

#[test]
fn an_endpoint_that_offers_the_model_holds_nothing_back() {
    let w = World::new("room-holds");
    w.mesh_curl();
    w.mesh(Some(r#"{"data":[{"id":"other"},{"id":"test-model"}]}"#));
    w.poll();
    assert_eq!(w.started(), "7 ");
    has(&w.calls("curl"), "http://mesh.test/v1/models");
    assert_eq!(w.dispatch_state()["why"], json!(""));
}

/// No cycle starts on `answer` (None: no answer at all), and the hold says
/// `text` once.
fn no_resources(answer: Option<&str>, text: &str) {
    let w = World::new("room-holds");
    w.mesh_curl();
    w.mesh(answer);
    let r = w.poll();
    assert_eq!(r.rc, 0, "{}", r.out);
    assert_eq!(w.started(), "");
    lacks(&w.gh(), "issue edit");
    let hold = format!("holding: no resources: {text}");
    assert_eq!(count(&r.out, &hold), 1, "{}", r.out);
    assert_eq!(
        jq("[.why, .hold]", &state_text(&w)),
        json!(["resources", hold]).to_string()
    );
    lacks(&state_text(&w), "mesh.test");
}

#[test]
fn no_resources_the_model_endpoint_does_not_answer() {
    no_resources(None, "the model endpoint does not answer");
}

#[test]
fn no_resources_the_model_endpoint_offers_no_models() {
    no_resources(Some("not json"), "the model endpoint offers no models");
    no_resources(
        Some(r#"{"data":[]}"#),
        "the model endpoint offers no models",
    );
}

#[test]
fn no_resources_the_model_endpoint_does_not_offer_test_model() {
    no_resources(
        Some(r#"{"data":[{"id":"other"}]}"#),
        "the model endpoint does not offer test-model",
    );
}

#[test]
fn no_resources_an_endpoint_that_refuses_the_request_is_said_to_with_its_http_code() {
    let w = World::new("room-holds");
    w.mesh(None);
    w.stub(
        "curl",
        r#"cat >/dev/null; echo "curl: (22) The requested URL returned error: 401" >&2; exit 22"#,
    );
    w.poll();
    assert_eq!(w.started(), "");
    assert_eq!(
        jq("[.why, .hold]", &state_text(&w)),
        r#"["resources","holding: no resources: the model endpoint refused the request with HTTP 401"]"#
    );
}

fn with(more: &[&'static str]) -> Vec<&'static str> {
    let mut v = MINIMAL.to_vec();
    v.push("TEHAS_MAX_CYCLES=1");
    v.extend_from_slice(more);
    v
}

#[test]
fn a_roles_own_model_the_endpoint_does_not_offer_holds_the_start_and_the_endpoint_is_asked_once() {
    let w = World::new("room-holds");
    w.mesh_curl();
    let small = with(&["PI_ENSEMBLE_MODEL_CODE_REVIEW_SPECIALIST=small-model"]);
    w.instance(&small);
    w.mesh(Some(OFFERS));
    let r = w.poll();
    assert_eq!(w.started(), "");
    assert_eq!(
        count(
            &r.out,
            "holding: no resources: the model endpoint does not offer small-model"
        ),
        1,
        "{}",
        r.out
    );
    let asks = w
        .calls("curl")
        .lines()
        .filter(|l| l.ends_with("/models"))
        .count();
    assert_eq!(asks, 1);
    w.mesh(Some(
        r#"{"data":[{"id":"test-model"},{"id":"small-model"}]}"#,
    ));
    w.reset();
    w.instance(&small);
    w.poll();
    assert_eq!(w.started(), "7 ");
}

const TWO_PROVIDERS: &str = r#"{"providers":{"test-provider":{"baseUrl":"http://mesh.test/v1"},"other":{"baseUrl":"http://other.test/v1"}}}"#;

fn models_json(w: &World, text: &str) {
    fs::write(
        w.t().join("home/.pi/agent/models.json"),
        format!("{text}\n"),
    )
    .unwrap();
}

#[test]
fn a_role_on_a_provider_of_its_own_is_asked_of_that_providers_endpoint() {
    let w = World::new("room-holds");
    w.mesh_curl();
    w.instance(&with(&[
        "PI_ENSEMBLE_MODEL_DEVELOPER=big-model",
        "PI_ENSEMBLE_PROVIDER_DEVELOPER=other",
    ]));
    w.mesh(Some(OFFERS));
    models_json(&w, TWO_PROVIDERS);
    let r = w.poll();
    has(&w.calls("curl"), "http://other.test/v1/models");
    assert_eq!(
        count(
            &r.out,
            "holding: no resources: the model endpoint does not offer big-model"
        ),
        1,
        "{}",
        r.out
    );
    lacks(&state_text(&w), "other.test");
}

/// The stand-in for curl of a main endpoint and a backup: main.test answers
/// from mesh.json and backup.test from backup.json.
const BACKED_CURL: &str = r####"conf=""
case " $* " in *" -K - "*) conf=$(cat) ;; esac
for a; do case "$a" in /dev/fd/*) cat "$a" >>"$T/calls/curl-config" ;; esac; done
if [ -n "$conf" ]; then
  printf "%s\n" "$conf" | grep -v "^data = " >>"$T/calls/curl-config"
  body=$(printf "%s\n" "$conf" | sed -n "s/^data = //p")
  [ -z "$body" ] || jq -r . <<<"$body" >"$T/calls/curl-body"
fi
heard() { [ -n "$conf" ] || cat >"$T/calls/curl-body"; }
case "$*" in
  *"/chat/completions") heard; [ -e "$T/fx/reads-images" ] ;;
  *backup.test*) cat "$T/fx/backup.json" 2>/dev/null ;;
  *) cat "$T/fx/mesh.json" 2>/dev/null ;;
esac"####;

/// A main endpoint and a backup, each with its own token, and the backup's
/// answer when one is given.
fn backed(w: &World, answer: Option<&str>) {
    fs::create_dir_all(w.t().join("home/.pi/agent")).unwrap();
    let v = json!({"providers": {"test-provider": {
        "baseUrl": "http://mesh.test/v1", "apiKey": "$TEHAS_MODEL_KEY_MAIN",
        "backupBaseUrl": "http://backup.test/v1",
        "backupApiKey": "$TEHAS_MODEL_KEY_BACKUP"}}});
    models_json(w, &v.to_string());
    w.rm_fx("backup.json");
    if let Some(a) = answer {
        w.fx("backup.json", &format!("{a}\n"));
    }
}

/// A world of the main endpoint and the backup, with both tokens set.
fn keyed() -> World {
    let w = World::new("room-holds");
    w.stub("curl", BACKED_CURL);
    keys(&w);
    w
}

fn keys(w: &World) {
    w.env("TEHAS_MODEL_KEY_MAIN", "main-secret");
    w.env("TEHAS_MODEL_KEY_BACKUP", "backup-secret");
}

fn curl_config(w: &World) -> String {
    fs::read_to_string(w.t().join("calls/curl-config")).unwrap_or_default()
}

#[test]
fn a_main_endpoint_that_offers_the_model_is_used_with_its_token_and_the_backup_is_not_asked() {
    let w = keyed();
    w.mesh(Some(OFFERS));
    backed(&w, Some(OFFERS));
    let r = w.poll();
    assert_eq!(w.started(), "7 ");
    lacks(&w.calls("curl"), "backup.test");
    assert_eq!(
        curl_config(&w).trim_end_matches('\n'),
        r#"header = "Authorization: Bearer main-secret""#
    );
    lacks(&w.calls("curl"), "secret");
    lacks(&r.out, "backup");
}

#[test]
fn a_main_endpoint_that_fails_starts_the_cycle_on_the_backup_and_says_so() {
    let w = keyed();
    for main in [None, Some(r#"{"data":[{"id":"other"}]}"#)] {
        w.reset();
        keys(&w);
        w.mesh(main);
        backed(&w, Some(OFFERS));
        let _ = fs::remove_file(w.t().join("calls/curl-config"));
        let r = w.poll();
        assert_eq!(w.started(), "7 ", "{}", r.out);
        has(&w.calls("curl"), "http://backup.test/v1/models");
        has(&curl_config(&w), "Bearer backup-secret");
        has(
            &r.out,
            "model endpoint: provider test-provider is on its backup",
        );
        assert_eq!(w.dispatch_state()["why"], json!(""));
    }
}

/// Which endpoint the subagent provider is on, as the energy sampler reads
/// it: the one word in live/dispatch.json. The endpoint itself is never
/// written.
#[test]
fn the_poll_publishes_the_subagent_providers_endpoint_as_one_word() {
    let w = keyed();
    w.mesh(Some(OFFERS));
    backed(&w, Some(OFFERS));
    w.poll();
    assert_eq!(w.dispatch_state()["endpoint"], json!("main"));
    lacks(&state_text(&w), "mesh.test");

    w.reset();
    keys(&w);
    w.mesh(Some(r#"{"data":[{"id":"other"}]}"#));
    backed(&w, Some(OFFERS));
    w.poll();
    assert_eq!(w.dispatch_state()["endpoint"], json!("backup"));
    lacks(&state_text(&w), "backup.test");

    // The word is missing whenever the poll asks nothing or gets no answer. A
    // provider with no backup is still main; a poll that cannot reach either
    // endpoint holds and writes no word.
    w.reset();
    keys(&w);
    w.mesh(Some(OFFERS));
    w.rm_fx("backup.json");
    w.poll();
    assert_eq!(w.dispatch_state()["endpoint"], json!("main"));
    assert!(w.dispatch_state().get("endpoint").is_some());

    w.reset();
    keys(&w);
    w.mesh(None);
    backed(&w, Some(r#"{"data":[]}"#));
    w.poll();
    assert_eq!(w.dispatch_state()["why"], json!("resources"));
    assert!(w.dispatch_state().get("endpoint").is_none());

    // A role on another provider does not move the word: the subagent's own
    // endpoint is the one published.
    w.reset();
    keys(&w);
    for (k, v) in [
        ("TEHAS_MAX_CYCLES", "1"),
        ("PI_ENSEMBLE_MODEL_DEVELOPER", "big-model"),
        ("PI_ENSEMBLE_PROVIDER_DEVELOPER", "other"),
    ] {
        w.env(k, v);
    }
    w.mesh(Some(OFFERS));
    backed(&w, Some(OFFERS));
    models_json(&w, TWO_PROVIDERS);
    w.poll();
    assert_eq!(w.dispatch_state()["endpoint"], json!("main"));
    lacks(&state_text(&w), "other.test");
}

#[test]
fn with_both_endpoints_failing_the_start_is_held_and_both_reasons_are_given() {
    let w = keyed();
    w.mesh(None);
    backed(&w, Some(r#"{"data":[]}"#));
    w.poll();
    assert_eq!(w.started(), "");
    assert_eq!(
        w.dispatch_state()["hold"],
        json!("holding: no resources: the model endpoint does not answer, and its backup offers no models")
    );
    lacks(&state_text(&w), "test/v1");
}

#[test]
fn a_token_models_json_names_and_the_host_has_not_set_stops_the_dispatcher_exit_78() {
    let w = World::new("room-holds");
    w.stub("curl", BACKED_CURL);
    w.env("TEHAS_MODEL_KEY_MAIN", "main-secret");
    backed(&w, Some(OFFERS));
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(
        &r.out,
        "models.json names TEHAS_MODEL_KEY_BACKUP and it is not set",
    );
    assert_eq!(format!("{}{}", w.calls("curl"), w.started()), "");
}

#[test]
fn providers_given_as_a_list_stop_the_dispatcher_exit_78() {
    let w = World::new("room-holds");
    w.mesh_curl();
    fs::create_dir_all(w.t().join("home/.pi/agent")).unwrap();
    models_json(&w, r#"{"providers": [{"apiKey": "$GH_TOKEN"}]}"#);
    let r = w.poll();
    assert_eq!(r.rc, 78, "{}", r.out);
    has(&r.out, "providers is not a map of provider names");
    assert_eq!(format!("{}{}", w.calls("curl"), w.started()), "");
}
