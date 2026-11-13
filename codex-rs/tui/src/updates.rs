// Modified for Arcvera-Core R41 (2026-11-11); see MODIFICATIONS.md.
// Modified for Arcvera-Core 1.0.1 (2026-11-13); see MODIFICATIONS.md.
// Modified for OpenDIY Codex R33 (2026-11-02); see MODIFICATIONS.md.
// Arcvera Core uses explicit release downloads; never consult official feeds.
#![cfg(not(debug_assertions))]

use crate::legacy_core::config::Config;
pub(crate) use crate::updates_cache::dismiss_version;

pub fn get_upgrade_version(_config: &Config) -> Option<String> {
    None
}

pub fn get_upgrade_version_for_popup(_config: &Config) -> Option<String> {
    None
}
