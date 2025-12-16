//! Shared memory (SHM) transport.

pub mod futex;
pub mod layout;
mod alloc;
mod session;
mod slot_guard;
mod transport;

pub use alloc::ShmAllocator;
pub use allocator_api2;
pub use session::{ShmSession, ShmSessionConfig};
pub use slot_guard::SlotGuard;
pub use transport::{ShmMetrics, ShmTransport};
