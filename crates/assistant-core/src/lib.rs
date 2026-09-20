//! The assistant owns decisions about execution. Models only propose actions.
mod context;
mod engine;
pub mod registry;
pub mod sms;
pub mod testing;
pub use engine::Assistant;
