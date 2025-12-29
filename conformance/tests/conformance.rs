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

/// Send a Hello frame as initiator.
async fn send_hello(transport: &StreamTransport) -> Result<(), String> {
    let hello = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Initiator,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: vec![],
        params: vec![],
    };

    let payload = facet_format_postcard::to_vec(&hello)
        .map_err(|e| format!("failed to encode Hello: {}", e))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = FrameFlags::from_bits_truncate(flags::CONTROL);

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(&payload);
        Frame {
            desc,
            payload: Payload::Inline,
        }
    } else {
        desc.payload_slot = 0;
        desc.payload_len = payload.len() as u32;
        Frame {
            desc,
            payload: Payload::Owned(payload),
        }
    };

    transport
        .send_frame(frame)
        .await
        .map_err(|e| format!("failed to send Hello: {}", e))
}

/// Receive and validate Hello response.
async fn recv_hello(transport: &StreamTransport) -> Result<(), String> {
    let frame = transport
        .recv_frame()
        .await
        .map_err(|e| format!("failed to receive Hello response: {}", e))?;

    trace!(
        channel_id = frame.desc.channel_id,
        method_id = frame.desc.method_id,
        flags = ?frame.desc.flags,
        payload_len = frame.desc.payload_len,
        "received frame"
    );

    if frame.desc.channel_id != 0 {
        return Err(format!(
            "expected Hello on channel 0, got channel {}",
            frame.desc.channel_id
        ));
    }

    if frame.desc.method_id != control_verb::HELLO {
        return Err(format!(
            "expected Hello (method_id=0), got method_id={}",
            frame.desc.method_id
        ));
    }

    // Decode and validate
    let hello: Hello = facet_format_postcard::from_slice(frame.payload_bytes())
        .map_err(|e| format!("failed to decode Hello: {}", e))?;

    trace!(?hello, "decoded Hello response");

    if hello.role != Role::Acceptor {
        return Err(format!(
            "expected Role::Acceptor in response, got {:?}",
            hello.role
        ));
    }

    Ok(())
}

/// Run the handshake.valid_hello_exchange test.
async fn run_handshake_valid_hello_exchange(bin_path: &str) -> Result<(), String> {
    let (mut child, transport) = spawn_harness(bin_path, "handshake.valid_hello_exchange").await?;

    // Send our Hello as initiator
    send_hello(&transport).await?;

    // Receive Hello response from harness
    recv_hello(&transport).await?;

    // Close transport and wait for child
    transport.close();

    let status = child
        .wait()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    if status.success() {
        Ok(())
    } else {
        Err(format!(
            "conformance test failed with exit code {:?}",
            status.code()
        ))
    }
}

/// Send an OpenChannel control message.
async fn send_open_channel(
    transport: &StreamTransport,
    channel_id: u32,
    kind: ChannelKind,
) -> Result<(), String> {
    let open = OpenChannel {
        channel_id,
        kind,
        attach: None,
        metadata: vec![],
        initial_credits: 1024 * 1024,
    };

    let payload = facet_format_postcard::to_vec(&open)
        .map_err(|e| format!("failed to encode OpenChannel: {}", e))?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 2; // After Hello
    desc.channel_id = 0; // Control channel
    desc.method_id = control_verb::OPEN_CHANNEL;
    desc.flags = FrameFlags::from_bits_truncate(flags::CONTROL);

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        desc.payload_slot = INLINE_PAYLOAD_SLOT;
        desc.payload_len = payload.len() as u32;
        desc.inline_payload[..payload.len()].copy_from_slice(&payload);
        Frame {
            desc,
            payload: Payload::Inline,
        }
    } else {
        desc.payload_slot = 0;
        desc.payload_len = payload.len() as u32;
        Frame {
            desc,
            payload: Payload::Owned(payload),
        }
    };

    transport
        .send_frame(frame)
        .await
        .map_err(|e| format!("failed to send OpenChannel: {}", e))
}

/// Run the call.one_req_one_resp test using RpcSession.
///
/// This tests that RpcSession.call() properly sends frames and receives responses.
async fn run_call_one_req_one_resp(bin_path: &str) -> Result<(), String> {
    let (child, transport) = spawn_harness(bin_path, "call.one_req_one_resp").await?;

    // 1. Send Hello (RpcSession doesn't do this)
    send_hello(&transport).await?;
    trace!("sent Hello");

    // 2. Receive Hello response
    recv_hello(&transport).await?;
    trace!("received Hello response");

    // 3. Send OpenChannel for channel 1 (RpcSession doesn't do this either)
    // RpcSession starts channel IDs at 1 (odd = initiator)
    let channel_id = 1u32;
    send_open_channel(&transport, channel_id, ChannelKind::Call).await?;
    trace!(channel_id, "sent OpenChannel");

    // 4. Create RpcSession and use it to make the call
    // The session will send DATA|EOS and wait for response
    let session = Arc::new(RpcSession::new(transport));

    // Spawn the run loop to receive the response
    let session_clone = session.clone();
    let run_handle = tokio::spawn(async move {
        let _ = session_clone.run().await;
    });

    // Make the call on the channel we already opened
    let method_id = rapace_protocol::compute_method_id("Test", "echo");
    let response = session
        .call(channel_id, method_id, b"test request".to_vec())
        .await
        .map_err(|e| format!("call failed: {:?}", e))?;

    trace!(
        channel_id = response.channel_id(),
        method_id = response.method_id(),
        flags = ?response.flags(),
        "received response via RpcSession"
    );

    // Validate response
    if response.channel_id() != channel_id {
        return Err(format!(
            "response on wrong channel: expected {}, got {}",
            channel_id,
            response.channel_id()
        ));
    }

    if !response.flags().contains(FrameFlags::RESPONSE) {
        return Err("response missing RESPONSE flag".to_string());
    }

    // Clean up
    session.close();
    run_handle.abort();

    let output = child
        .wait_with_output()
        .await
        .map_err(|e| format!("failed to wait for child: {}", e))?;

    let stderr = String::from_utf8_lossy(&output.stderr);
    if !stderr.is_empty() {
        trace!("harness stderr: {}", stderr);
    }

    if output.status.success() {
        Ok(())
    } else {
        Err(format!(
            "conformance test failed with exit code {:?}, stderr: {}",
            output.status.code(),
            stderr
        ))
    }
}

fn main() {
    let args = Arguments::from_args();

    // Get the path to the conformance binary
    let conformance_bin = env!("CARGO_BIN_EXE_rapace-conformance");

    let mut trials = Vec::new();

    // handshake.valid_hello_exchange
    {
        let bin_path = conformance_bin.to_string();
        trials.push(Trial::test("handshake.valid_hello_exchange", move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
            rt.block_on(async {
                run_handshake_valid_hello_exchange(&bin_path)
                    .await
                    .map_err(Failed::from)
            })
        }));
    }

    // call.one_req_one_resp
    {
        let bin_path = conformance_bin.to_string();
        trials.push(Trial::test("call.one_req_one_resp", move || {
            let rt = tokio::runtime::Runtime::new().expect("failed to create runtime");
            rt.block_on(async {
                run_call_one_req_one_resp(&bin_path)
                    .await
                    .map_err(Failed::from)
            })
        }));
    }

    libtest_mimic::run(&args, trials).exit();
}
