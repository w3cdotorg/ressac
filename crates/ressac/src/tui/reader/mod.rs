//! The reading interface (sub-project 2b): `ressac` without a subcommand and `ressac read`.
//! Its pieces arrive phase by phase and are wired into the CLI in phase 5b; until then they are
//! exercised by their tests only, hence the dead-code allowance (removed in phase 5b).
#![cfg_attr(
    not(test),
    allow(
        dead_code,
        reason = "wired into the event loop and the CLI in phase 5b"
    )
)]

pub mod html;
