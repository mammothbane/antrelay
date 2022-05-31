#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::*;

#[cfg(unix)]
pub mod unix;

#[cfg(unix)]
pub use unix::*;
