//! Conformance tests using real rapace-core implementation.
//!
//! This test harness runs conformance tests from the rapace-conformance binary.
//! It spawns the conformance harness and connects to it using a real StreamTransport,
//! letting rapace-core handle the protocol.

use std::process::Stdio;
use std::sync::Arc;

use libtest_mimic::{Arguments, Failed, Trial};
use tokio::process::{Child, Command as TokioCommand};
use tracing::trace;

use rapace_core::stream::StreamTransport;
use rapace_core::{BufferPool, Frame, FrameFlags, MsgDescHot, Payload, RpcSession, Transport};
use rapace_protocol::{
    ChannelKind, Hello, INLINE_PAYLOAD_SIZE, INLINE_PAYLOAD_SLOT, Limits, OpenChannel,
    PROTOCOL_VERSION_1_0, Role, control_verb, features, flags,
};

/// Wrapper to make ChildStdin/ChildStdout work with StreamTransport.
struct ChildIo {
    stdin: tokio::process::ChildStdin,
    stdout: tokio::process::ChildStdout,
}

impl tokio::io::AsyncRead for ChildIo {
    fn poll_read(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &mut tokio::io::ReadBuf<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdout).poll_read(cx, buf)
    }
}

impl tokio::io::AsyncWrite for ChildIo {
    fn poll_write(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
        buf: &[u8],
    ) -> std::task::Poll<std::io::Result<usize>> {
        std::pin::Pin::new(&mut self.stdin).poll_write(cx, buf)
    }

    fn poll_flush(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_flush(cx)
    }

    fn poll_shutdown(
        mut self: std::pin::Pin<&mut Self>,
        cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<std::io::Result<()>> {
        std::pin::Pin::new(&mut self.stdin).poll_shutdown(cx)
    }
}

/// Spawn the conformance harness and create a transport connected to it.
async fn spawn_harness(
    bin_path: &str,
    test_case: &str,
) -> Result<(Child, StreamTransport), String> {
    let mut child = TokioCommand::new(bin_path)
        .args(["--case", test_case])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped()) // Capture stderr so we can see harness errors
        .spawn()
        .map_err(|e| format!("failed to spawn conformance binary: {}", e))?;

    let stdin = child.stdin.take().ok_or("failed to get stdin")?;
    let stdout = child.stdout.take().ok_or("failed to get stdout")?;

    let io = ChildIo { stdin, stdout };
    let transport = StreamTransport::with_buffer_pool(io, BufferPool::new());

    Ok((child, transport))
}

fn main() {
    let args = Arguments::from_args();

    // Get the path to the conformance binary
    // When run via `cargo run`, we find it relative to our own executable
    let conformance_bin = std::env::current_exe()
        .expect("failed to get current exe")
        .parent()
        .expect("exe has no parent dir")
        .join("rapace-conformance")
        .to_string_lossy()
        .to_string();

    let mut trials = Vec::new();

    panic!(
        "every test should just run conformance-runner against THE RUST IMPLEMENTATION OF RAPACE. NOT THIS HARNESS. LET'S TALK ABOUT THIS IN PLAN MODE"
    );

    libtest_mimic::run(&args, trials).exit();
}
