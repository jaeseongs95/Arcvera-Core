// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
#![allow(clippy::expect_used)]

// Keep the existing case paths and include its original module without copying it.
mod suite {
    mod v2 {
        mod initialize {
            include!(concat!(env!("CARGO_MANIFEST_DIR"), "/tests/suite/v2/initialize.rs"));
        }
    }
}
