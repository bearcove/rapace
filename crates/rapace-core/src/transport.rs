//! Transport enum and internal backend trait.
//!
//! The public API is the [`Transport`] enum. Each backend lives in its own
//! module under `transport/` and implements the internal [`TransportBackend`]
//! trait. We use `enum_dispatch` to forward calls without handwritten `match`
//! boilerplate.

use enum_dispatch::enum_dispatch;

use crate::{Frame, TransportError};

#[enum_dispatch]
pub(crate) trait TransportBackend: Send + Sync + Clone + 'static {
    async fn send_frame(&self, frame: Frame) -> Result<(), TransportError>;
    async fn recv_frame(&self) -> Result<Frame, TransportError>;
    fn close(&self);
    fn is_closed(&self) -> bool;
}

#[enum_dispatch(TransportBackend)]
#[derive(Clone, Debug)]
pub enum Transport {
    #[cfg(feature = "mem")]
    Mem(mem::MemTransport),
    #[cfg(feature = "stream")]
    Stream(stream::StreamTransport),
    #[cfg(feature = "shm")]
    Shm(shm::ShmTransport),
    #[cfg(feature = "websocket")]
    WebSocket(websocket::WebSocketTransport),
}

impl Transport {
    pub async fn send_frame(&self, frame: Frame) -> Result<(), TransportError> {
        TransportBackend::send_frame(self, frame).await
    }

    pub async fn recv_frame(&self) -> Result<Frame, TransportError> {
        TransportBackend::recv_frame(self).await
    }

    pub fn close(&self) {
        TransportBackend::close(self);
    }

    pub fn is_closed(&self) -> bool {
        TransportBackend::is_closed(self)
    }

    #[cfg(feature = "mem")]
    pub fn inproc_pair() -> (Self, Self) {
        let (a, b) = mem::MemTransport::pair();
        (Transport::Mem(a), Transport::Mem(b))
    }
}

#[cfg(feature = "mem")]
pub mod mem;
#[cfg(feature = "shm")]
pub mod shm;
#[cfg(feature = "stream")]
pub mod stream;
#[cfg(feature = "websocket")]
pub mod websocket;
