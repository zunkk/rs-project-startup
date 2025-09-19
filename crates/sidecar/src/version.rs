use std::sync::OnceLock;

#[derive(Debug, Clone, Copy)]
pub struct Version {
    pub app_name: &'static str,
    pub app_desc: &'static str,
    pub app_authors: &'static str,
    pub version: &'static str,
    pub git_branch: &'static str,
    pub git_commit: &'static str,
    pub build_time: &'static str,
}

const DEFAULT_VERSION: Version = Version {
    app_name: "unknown",
    app_desc: "unknown",
    app_authors: "unknown",
    version: "unknown",
    git_branch: "unknown",
    git_commit: "unknown",
    build_time: "unknown",
};

impl Default for Version {
    fn default() -> Self {
        DEFAULT_VERSION
    }
}

static VERSION: OnceLock<Version> = OnceLock::new();

pub fn init(version: Version) {
    let _ = VERSION.set(version);
}

pub fn current() -> &'static Version {
    VERSION.get().unwrap_or(&DEFAULT_VERSION)
}
