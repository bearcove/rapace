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
    pub fn mem_pair() -> (Self, Self) {
        let (a, b) = mem::MemTransport::pair();
        (Transport::Mem(a), Transport::Mem(b))
    }

    #[cfg(feature = "shm")]
    pub fn shm(session: std::sync::Arc<shm::ShmSession>) -> Self {
        Transport::Shm(shm::ShmTransport::new(session))
    }

    #[cfg(feature = "shm")]
    pub fn shm_pair() -> (Self, Self) {
        let (a, b) = shm::ShmTransport::pair().expect("failed to create shm transport pair");
        (Transport::Shm(a), Transport::Shm(b))
    }

    #[cfg(feature = "stream")]
    pub fn stream<S>(stream: S) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + Sync + 'static,
    {
        Transport::Stream(stream::StreamTransport::new(stream))
    }

    #[cfg(feature = "stream")]
    pub fn stream_pair() -> (Self, Self) {
        let (a, b) = stream::StreamTransport::pair();
        (Transport::Stream(a), Transport::Stream(b))
    }

    #[cfg(all(feature = "websocket-tungstenite", not(target_arch = "wasm32")))]
    pub fn websocket<S>(ws: tokio_tungstenite::WebSocketStream<S>) -> Self
    where
        S: tokio::io::AsyncRead + tokio::io::AsyncWrite + Unpin + Send + 'static,
    {
        Transport::WebSocket(websocket::WebSocketTransport::new(ws))
    }

    #[cfg(all(feature = "websocket-tungstenite", not(target_arch = "wasm32")))]
    pub async fn websocket_pair() -> (Self, Self) {
        let (a, b) = websocket::WebSocketTransport::pair().await;
        (Transport::WebSocket(a), Transport::WebSocket(b))
    }

    #[cfg(all(feature = "websocket-axum", not(target_arch = "wasm32")))]
    pub fn websocket_axum(ws: axum::extract::ws::WebSocket) -> Self {
        Transport::WebSocket(websocket::WebSocketTransport::from_axum(ws))
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
