mod actions;
mod automation;
mod com;
mod failure;
mod fingerprint;
mod handle;
mod lookup;
mod options;
mod properties;
mod refind;
mod registry;
mod requests;
mod resolved;
mod scroll;
mod select_option;
mod snapshot;
mod text;
mod tree;
mod variant;
mod win;

#[cfg(test)]
mod test_support;

pub use automation::Uia;
pub use handle::UiaHandle;
pub use options::{SnapshotOptions, UiaOptions};
