use crate::{Frame, TransportError};

use super::TransportBackend;

#[derive(Clone, Debug)]
pub struct ShmTransport;

impl TransportBackend for ShmTransport {
    async fn send_frame(&self, _frame: Frame) -> Result<(), TransportError> {
        todo!("shm transport not ported yet")
    }

    async fn recv_frame(&self) -> Result<Frame, TransportError> {
        todo!("shm transport not ported yet")
    }

    fn close(&self) {
        todo!("shm transport not ported yet")
    }

    fn is_closed(&self) -> bool {
        todo!("shm transport not ported yet")
    }
}
