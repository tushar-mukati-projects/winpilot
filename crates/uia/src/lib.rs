mod actions;
mod ancestry;
mod automation;
mod com;
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
mod select_option;
mod snapshot;
mod text;
mod toggle;
mod tree;
mod tree_path;
mod upkeep;
mod variant;
mod warmed;
mod win;
mod window_identity;

#[cfg(test)]
mod test_support;

pub use automation::Uia;
pub use handle::UiaHandle;
pub use options::{SnapshotOptions, UiaOptions};
