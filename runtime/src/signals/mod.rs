#[cfg(windows)]
pub mod windows;

#[cfg(windows)]
pub use windows::WindowsSignal as Signal;

#[cfg(unix)]
pub mod unix;

#[cfg(unix)]
pub use unix::UnixSignal as Signal;

#[derive(actix::Message)]
#[rtype(result = "()")]
pub struct Term;
