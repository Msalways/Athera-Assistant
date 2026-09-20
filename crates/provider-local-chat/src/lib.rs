//! Offline conversation through a pinned native `llama.cpp` executable.
mod model;
#[cfg(test)]
mod model_tests;
#[cfg(feature = "native-ffi")]
#[allow(unsafe_code)]
mod native;
mod provider;

pub use model::{default_manifest, InstallationSink, ModelError, ModelManager, QWEN3_1_7B_Q4_K_M};
#[cfg(feature = "native-ffi")]
pub use native::NativeLocalChatProvider;
pub use provider::LocalChatProvider;
