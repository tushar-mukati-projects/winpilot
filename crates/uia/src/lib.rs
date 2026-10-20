mod actions;
mod ancestry;
mod automation;
mod cached_tree;
mod failure;
mod fingerprint;
mod handle;
mod lookup;
mod options;
mod pattern;
mod properties;
mod refind;
mod registry;
mod requests;
mod resolution;
mod resolved;
mod scroll;
mod search;
mod select_option;
mod snapshot;
mod text;
mod toggle;
mod tree_path;
mod variant;
mod warmed;
mod win32_combo;
mod window_identity;

#[cfg(test)]
mod test_support;

pub use automation::Uia;
pub use handle::UiaHandle;
pub use options::{SnapshotOptions, UiaOptions};
