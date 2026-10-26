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
    pub fixer: String,
    pub concurrency: i64,
    pub max_awaiting: i64,
    pub reserve: String,
    pub refresh_conflicts: bool,
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
    /// PI_ENSEMBLE_SUBAGENT_MODEL and PI_ENSEMBLE_IMAGE.
    pub model: String,
    pub image: String,
    pub refresh_max: i64,
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
            fixer: s("TEHAS_FIXER_LOGIN", ""),
            concurrency,
            max_awaiting: n("TEHAS_MAX_AWAITING", concurrency + 2),
            reserve: s("TEHAS_CYCLE_RESERVE", "2g"),
            refresh_conflicts: s("TEHAS_REFRESH_CONFLICTS", "0") == "1",
            refresh_slots: n("TEHAS_REFRESH_SLOTS", 1),
            min_free,
            min_free_mb,
            branch_check: s("TEHAS_BRANCH_CHECK", ""),
            branch: s("TEHAS_BRANCH", "main"),
            low_gate: s("TEHAS_LOW_GATE", ""),
            fixer_max_retries: n("TEHAS_FIXER_MAX_RETRIES", 2),
            size_max_files: v
                .get("TEHAS_SIZE_MAX_FILES")
                .and_then(|x| x.as_u64().or_else(|| x.as_str()?.parse().ok())),
            size_max_bytes: n("TEHAS_SIZE_MAX_BYTES", 131_072) as usize,
            size_max_lines: n("TEHAS_SIZE_MAX_LINES", 1000) as usize,
            work_images: s("TEHAS_WORK_IMAGES", "0") == "1",
            model: s("PI_ENSEMBLE_SUBAGENT_MODEL", ""),
            image: s("PI_ENSEMBLE_IMAGE", ""),
            refresh_max: n("TEHAS_REFRESH_MAX", 2),
        }
    }

    /// tehas_allowed_labeler: an empty name is nobody.
    pub fn allowed(&self, login: &str) -> bool {
        !login.is_empty() && self.allowed.iter().any(|a| a == login)
    }

    /// tehas_fixer_is.
    pub fn is_fixer(&self, login: &str) -> bool {
        !self.fixer.is_empty() && login == self.fixer
    }
}
