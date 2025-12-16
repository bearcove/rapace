use crate::{Frame, TransportError};

use super::TransportBackend;

#[derive(Clone, Debug)]
pub struct WebSocketTransport;

impl TransportBackend for WebSocketTransport {
    async fn send_frame(&self, _frame: Frame) -> Result<(), TransportError> {
        todo!("websocket transport not ported yet")
    }

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        todo!("websocket transport not ported yet")
    }

    fn close(&self) {
        todo!("websocket transport not ported yet")
    }

    fn is_closed(&self) -> bool {
        todo!("websocket transport not ported yet")
    }
}
