//! ressac-kde: Ressac for KDE Plasma. It builds everywhere so `cargo test --workspace` needs no
//! Qt; off Linux it is a stub.

use std::process::ExitCode;

#[cfg(not(target_os = "linux"))]
fn main() -> ExitCode {
    eprintln!("ressac-kde runs on Linux only.");
    ExitCode::from(1)
}

#[cfg(target_os = "linux")]
fn main() -> ExitCode {
    ressac_kde::run()
}
