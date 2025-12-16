//! Shared memory (SHM) transport.

pub mod futex;
pub mod layout;
mod session;
mod transport;

pub use session::{ShmSession, ShmSessionConfig};
pub use transport::{ShmMetrics, ShmTransport};

/// Placeholder guard for SHM-backed payloads.
///
/// This will eventually be a real slot guard that:
/// - Derefs to a shared-memory byte slice
/// - Frees the slot on drop
#[derive(Debug)]
pub struct SlotGuard;

impl AsRef<[u8]> for SlotGuard {
    fn as_ref(&self) -> &[u8] {
        todo!("SlotGuard not implemented yet")
    }
}
