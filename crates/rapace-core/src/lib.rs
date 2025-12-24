#![doc = include_str!("../README.md")]
#![forbid(unsafe_op_in_unsafe_fn)]

mod buffer_pool;
mod control;
mod descriptor;
mod encoding;
mod error;
mod flags;
mod frame;
mod header;
mod limits;
mod session;
mod streaming;
mod transport;
#[cfg(not(target_arch = "wasm32"))]
mod tunnel_stream;
mod validation;

pub use buffer_pool::*;
pub use control::*;
pub use descriptor::*;
pub use encoding::*;
pub use error::*;
pub use flags::*;
pub use frame::*;
pub use header::*;
pub use limits::*;
pub use session::*;
pub use streaming::*;
pub use transport::*;
#[cfg(not(target_arch = "wasm32"))]
pub use tunnel_stream::*;
pub use validation::*;

// Re-export StreamExt for use by macro-generated streaming clients
pub use futures::StreamExt;

// Re-export try_stream for use by macro-generated streaming clients
pub use async_stream::try_stream;

/// Trait for service servers that can be dispatched
pub trait ServiceDispatch: Send + Sync + 'static {
    /// Returns the method IDs that this service handles.
    ///
    /// This is used by the dispatcher to build an O(1) lookup table at registration time,
    /// avoiding the need to try each service in sequence at dispatch time.
    fn method_ids(&self) -> &'static [u32];

    /// Dispatch a method call to this service
    fn dispatch(
        &self,
        method_id: u32,
        frame: Frame,
        buffer_pool: &BufferPool,
    ) -> impl Future<Output = Result<Frame, RpcError>> + Send;
}
