// Modified for Arcvera-Core 1.1.0 (2026-11-14); see MODIFICATIONS.md.
mod history;
mod image_availability;
mod normalize;
pub(crate) mod updates;

pub(crate) use history::ContextManager;
pub(crate) use history::HistoryReplacement;
pub(crate) use history::composition::CompositionIdentity;
pub(crate) use history::composition::HistoryComposition;
pub use history::composition::{CompositionFragment, CompositionSizes};
pub(crate) use history::estimate_image_reference_bytes;
pub(crate) use history::estimate_item_token_count;
pub(crate) use history::is_user_turn_boundary;
pub(crate) use image_availability::replace_unresolvable_image_files;
