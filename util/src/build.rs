macro_rules! env_or_none {
    ($name:ident, $env:literal) => {
        #[allow(dead_code)]
        pub const $name: &'static str = option_env!($env).unwrap_or("<none>");
    };
}

#[allow(dead_code)]
pub const PACKAGE: &str = "antrelay";
env_or_none!(VERSION, "VERGEN_BUILD_SEMVER");
env_or_none!(COMMIT_HASH, "VERGEN_GIT_SHA");
env_or_none!(BUILD_TIMESTAMP, "VERGEN_BUILD_TIMESTAMP");
env_or_none!(RUSTC_COMMIT_HASH, "VERGEN_RUSTC_COMMIT_HASH");
