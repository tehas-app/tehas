//! The instance settings a poll decides by, as lib/config.sh leaves them.

use serde_json::Value;

#[derive(Clone, Debug)]
pub struct Settings {
    /// TEHAS_HOME and the instance directory, as messages name them.
    pub home: String,
    pub instance_dir: String,
    pub repo: String,
    pub control: u64,
    pub allowed: Vec<String>,
    /// TEHAS_BOT_LOGIN: an app's login, which GET /user cannot answer.
    pub bot_login: String,
    /// TEHAS_MILLWRIGHT_LOGIN: the millwright's account, as `account` has it.
    pub millwright: String,
    pub concurrency: i64,
    /// TEHAS_TICKET_CONCURRENCY as written: `auto` derives it in slots.rs.
    pub concurrency_written: String,
    pub max_awaiting: i64,
    pub reserve: String,
    /// TEHAS_SLICE_CONTAINERS as written: slice::usable refuses one that is
    /// not a count, and slots.rs derives `auto` from the host.
    pub slice_containers: String,
    /// What one container is given when `TEHAS_SLICE_CONTAINERS=auto`, and
    /// what one cycle is given when `TEHAS_TICKET_CONCURRENCY=auto`.
    pub container_memory: String,
    pub cycle_cpus: String,
    /// The most cycles `auto` may take, so a big host does not outstrip the
    /// model endpoint. Never unset: config.sh's default is slots::MAX.
    pub cycle_ceiling: String,
    pub refresh_conflicts: bool,
    /// TEHAS_EASY_MERGE: 1 lets the dispatcher choose an easy merge for a
    /// refresh start instead of the full cycle. Off unless the instance says
    /// so; with it off every refresh is a full refresh as before (#1031).
    pub easy_merge: bool,
    pub refresh_slots: i64,
    /// TEHAS_CYCLE_MIN_FREE as written, and in MiB.
    pub min_free: String,
    pub min_free_mb: u64,
    pub branch_check: String,
    pub branch: String,
    pub low_gate: String,
    pub fixer_max_retries: i64,
    /// TEHAS_SIZE_MAX_FILES, None when unset, and the measure's bounds.
    pub size_max_files: Option<u64>,
    pub size_max_bytes: usize,
    pub size_max_lines: usize,
    pub work_images: bool,
    /// TEHAS_WORK_FILES: 1 gives a cycle the text files its issue links, in the
    /// directory `TEHAS_WORK_IMAGES` uses too. With it or `work_images` the
    /// record's `retry_context` reaches a cycle as `retry-context.md`; else
    /// where the instance gives a cycle neither, a millwright's comment of
    /// guidance reaches none, and the park's hold of the unchanged body stays
    /// (#1130).
    pub work_files: bool,
    /// PI_ENSEMBLE_SUBAGENT_MODEL and PI_ENSEMBLE_IMAGE.
    pub model: String,
    pub image: String,
    pub refresh_max: i64,
    pub fixer_max_plan_answers: i64,
    /// TEHAS_MILLWRIGHT_FEED: the millwright's tehas:ready may start the first
    /// cycle of an issue a person wrote. TEHAS_MILLWRIGHT_MAX_FEEDS: how many
    /// cycles so started may run at once.
    pub millwright_feed: bool,
    pub millwright_max_feeds: i64,
    /// Where the poll's libexec is, as the refine brief's path names it.
    pub libexec: String,
}

impl Settings {
    /// From the settings' names; a missing one has lib/config.sh's default.
    pub fn parse(v: &Value) -> Settings {
        let s = |k: &str, d: &str| v.get(k).and_then(Value::as_str).unwrap_or(d).to_string();
        let n = |k: &str, d: i64| {
            v.get(k)
                .and_then(|x| x.as_i64().or_else(|| x.as_str()?.parse().ok()))
                .unwrap_or(d)
        };
        let concurrency = n("TEHAS_TICKET_CONCURRENCY", 7);
        let min_free = s("TEHAS_CYCLE_MIN_FREE", "10g");
        // The two settings as written, so a poll can tell `auto` from a
        // number: a missing one is the number config.sh would have exported.
        let written = |k: &str, d: i64| {
            v.get(k).map_or(d.to_string(), |x| {
                x.as_str().map_or(x.to_string(), str::to_string)
            })
        };
        let min_free_mb = crate::slice::bytes(&min_free)
            .map(|b| b / 1_048_576)
            .unwrap_or(0);
        Settings {
            home: s("TEHAS_HOME", "/home/tehas/tehas"),
            instance_dir: s("TEHAS_INSTANCE_DIR", "/home/tehas/tehas/instance"),
            repo: s("TEHAS_REPO", "acme/widget"),
            control: n("TEHAS_CONTROL_ISSUE", 1) as u64,
            allowed: s("TEHAS_ALLOWED_LABELERS", "")
                .split(' ')
                .filter(|l| !l.is_empty())
                .map(str::to_string)
                .collect(),
            bot_login: s("TEHAS_BOT_LOGIN", ""),
            millwright: account(&s("TEHAS_MILLWRIGHT_LOGIN", "")).to_string(),
            concurrency,
            concurrency_written: written("TEHAS_TICKET_CONCURRENCY", 7),
            max_awaiting: n("TEHAS_MAX_AWAITING", concurrency + 2),
            reserve: s("TEHAS_CYCLE_RESERVE", "2g"),
            slice_containers: written("TEHAS_SLICE_CONTAINERS", 4),
            container_memory: s("TEHAS_CONTAINER_MEMORY", "8g"),
            cycle_cpus: s("TEHAS_CYCLE_CPUS", "4"),
            cycle_ceiling: s("TEHAS_CYCLE_CEILING", "16"),
            refresh_conflicts: s("TEHAS_REFRESH_CONFLICTS", "0") == "1",
            easy_merge: s("TEHAS_EASY_MERGE", "0") == "1",
            refresh_slots: n("TEHAS_REFRESH_SLOTS", 1),
            min_free,
            min_free_mb,
            branch_check: s("TEHAS_BRANCH_CHECK", ""),
            branch: s("TEHAS_BRANCH", "main"),
            low_gate: s("TEHAS_LOW_GATE", ""),
            fixer_max_retries: n("TEHAS_MILLWRIGHT_MAX_RETRIES", 2),
            size_max_files: v
                .get("TEHAS_SIZE_MAX_FILES")
                .and_then(|x| x.as_u64().or_else(|| x.as_str()?.parse().ok())),
            size_max_bytes: n("TEHAS_SIZE_MAX_BYTES", 131_072) as usize,
            size_max_lines: n("TEHAS_SIZE_MAX_LINES", 1000) as usize,
            work_images: s("TEHAS_WORK_IMAGES", "0") == "1",
            work_files: s("TEHAS_WORK_FILES", "0") == "1",
            model: s("PI_ENSEMBLE_SUBAGENT_MODEL", ""),
            image: s("PI_ENSEMBLE_IMAGE", ""),
            refresh_max: n("TEHAS_REFRESH_MAX", 2),
            fixer_max_plan_answers: n("TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS", 3),
            millwright_feed: s("TEHAS_MILLWRIGHT_FEED", "0") == "1",
            millwright_max_feeds: n("TEHAS_MILLWRIGHT_MAX_FEEDS", 2),
            libexec: s("libexec", "/home/tehas/tehas/libexec"),
        }
    }

    /// tehas_allowed_labeler: an empty name is nobody.
    pub fn allowed(&self, login: &str) -> bool {
        !login.is_empty() && self.allowed.iter().any(|a| a == login)
    }

    /// tehas_millwright_is.
    pub fn is_millwright(&self, login: &str) -> bool {
        !self.millwright.is_empty() && account(login) == self.millwright
    }
}

/// A login as GraphQL gives it: REST names a GitHub App `<slug>[bot]` and
/// GraphQL `<slug>`, one account.
pub fn account(login: &str) -> &str {
    login.strip_suffix("[bot]").unwrap_or(login)
}

#[cfg(test)]
mod tests {
    use super::Settings;
    use serde_json::json;

    #[test]
    fn the_millwrights_settings_take_only_the_new_names() {
        let new = Settings::parse(&json!({
            "TEHAS_MILLWRIGHT_LOGIN": "mill-bot",
            "TEHAS_MILLWRIGHT_MAX_RETRIES": "5",
            "TEHAS_MILLWRIGHT_MAX_PLAN_ANSWERS": 4,
        }));
        assert_eq!(
            (
                new.millwright.as_str(),
                new.fixer_max_retries,
                new.fixer_max_plan_answers
            ),
            ("mill-bot", 5, 4)
        );
        let old = Settings::parse(&json!({
            "TEHAS_FIXER_LOGIN": "mill-bot",
            "TEHAS_FIXER_MAX_RETRIES": "7",
            "TEHAS_FIXER_MAX_PLAN_ANSWERS": "6",
        }));
        assert_eq!(
            (
                old.millwright.as_str(),
                old.fixer_max_retries,
                old.fixer_max_plan_answers
            ),
            ("", 2, 3)
        );
        let none = Settings::parse(&json!({}));
        assert_eq!(
            (none.fixer_max_retries, none.fixer_max_plan_answers),
            (2, 3)
        );
    }

    #[test]
    fn the_lift_needs_images_or_files_and_no_other_setting_gives_it() {
        // Neither: a millwright's comment of guidance reaches no cycle, so the
        // park's hold stays, and the lift is off.
        let none = Settings::parse(&json!({}));
        assert!(!(none.work_images || none.work_files));
        // Either one is enough: both write retry-context.md in the directory
        // the instance already mounts.
        let images = Settings::parse(&json!({"TEHAS_WORK_IMAGES": "1"}));
        assert!(images.work_images && !images.work_files);
        let files = Settings::parse(&json!({"TEHAS_WORK_FILES": "1"}));
        assert!(files.work_files && !files.work_images);
        // Only `1` is on, as for every setting of this family.
        let other = Settings::parse(&json!({"TEHAS_WORK_FILES": "yes"}));
        assert!(!other.work_files);
    }

    #[test]
    fn the_feed_is_off_unless_it_is_1() {
        let none = Settings::parse(&json!({}));
        assert_eq!(
            (none.millwright_feed, none.millwright_max_feeds),
            (false, 2)
        );
        let on = Settings::parse(&json!({
            "TEHAS_MILLWRIGHT_FEED": "1",
            "TEHAS_MILLWRIGHT_MAX_FEEDS": "3",
        }));
        assert_eq!((on.millwright_feed, on.millwright_max_feeds), (true, 3));
        let other = Settings::parse(&json!({"TEHAS_MILLWRIGHT_FEED": "yes"}));
        assert!(!other.millwright_feed);
    }

    #[test]
    fn an_app_millwright_is_itself_in_both_forms_and_no_other_account() {
        for setting in ["acme-millwright", "acme-millwright[bot]"] {
            let c = Settings::parse(&json!({"TEHAS_MILLWRIGHT_LOGIN": setting}));
            let is: Vec<bool> = [
                "acme-millwright",
                "acme-millwright[bot]",
                "acme-millwright[bot][bot]",
                "acme-millwright2",
                "acme-millwright2[bot]",
                "acme-millwrigh",
                "[bot]",
                "",
            ]
            .iter()
            .map(|l| c.is_millwright(l))
            .collect();
            assert_eq!(
                is,
                [true, true, false, false, false, false, false, false],
                "{setting}"
            );
            assert_eq!(c.millwright, "acme-millwright", "{setting}");
        }
        let none = Settings::parse(&json!({}));
        assert!(!none.is_millwright("") && !none.is_millwright("[bot]"));
    }
}
