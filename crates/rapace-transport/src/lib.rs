//! Unified transports for rapace.
//!
//! This crate is intentionally minimal right now: it defines an enum that will
//! eventually wrap every transport implementation (mem, stream, shm, websocket)
//! under one API surface. The module layout mirrors the existing crates so we
//! can move code over incrementally without forcing downstream users to juggle
//! feature flags across multiple packages.

use rapace_core::{RecvFrame, SendFrame, TransportError, TransportHandle};

/// Combined transport handle. Each variant will embed the concrete handle once
/// the implementation is moved into this crate.
#[derive(Clone)]
pub enum Transport {
    /// In-process transport (enabled via the `mem` feature).
    #[cfg(feature = "mem")]
    Mem(mem::MemTransportNotYetMerged),
    /// Stream transport (enabled via the `stream` feature).
    #[cfg(feature = "stream")]
    Stream(stream::StreamTransportNotYetMerged),
    /// Shared-memory transport (enabled via the `shm` feature).
    #[cfg(feature = "shm")]
    Shm(shm::ShmTransportNotYetMerged),
    /// WebSocket transport (enabled via the `websocket` feature).
    #[cfg(feature = "websocket")]
    WebSocket(websocket::WebSocketTransportNotYetMerged),
}

impl TransportHandle for Transport {
    type SendPayload = Vec<u8>;
    type RecvPayload = Vec<u8>;

    async fn send_frame(
        &self,
        _frame: impl Into<rapace_core::SendFrame<Self::SendPayload>> + Send + 'static,
    ) -> Result<(), rapace_core::TransportError> {
        unimplemented!("transport enum send_frame stub")
    }

    async fn recv_frame(
        &self,
    ) -> Result<rapace_core::RecvFrame<Self::RecvPayload>, rapace_core::TransportError> {
        unimplemented!("transport enum recv_frame stub")
    }

    fn close(&self) {
        unimplemented!("transport enum close stub")
    }

    fn is_closed(&self) -> bool {
        false
    }
}

/// Module placeholders – real implementations will be migrated here.
#[cfg(feature = "mem")]
pub mod mem;
#[cfg(feature = "shm")]
pub mod shm;
#[cfg(feature = "stream")]
pub mod stream;
#[cfg(feature = "websocket")]
pub mod websocket;
