use vergen::{
    vergen,
    Config,
};

fn main() {
    vergen(Config::default()).unwrap_or_else(|e| {
        eprintln!("vergen failed: {}", e);
    });
}
