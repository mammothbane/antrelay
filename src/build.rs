macro_rules! env_or_none {
    ($name:ident, $env:literal) => {
        pub const $name: &'static str = option_env!($env).unwrap_or("<none>");
    };
}

env_or_none!(PACKAGE, "CARGO_PKG");
env_or_none!(VERSION, "CARGO_VERSION");
env_or_none!(COMMIT_HASH, "VERGEN_GIT_SHA");
env_or_none!(BUILD_TIMESTAMP, "VERGEN_BUILD_SEMVER");
env_or_none!(RUSTC_COMMIT_HASH, "VERGEN_RUSTC_COMMIT_HASH");
