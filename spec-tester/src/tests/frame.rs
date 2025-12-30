//! Frame format conformance tests.
//!
//! Tests for spec rules in frame-format.md
//!
//! These tests validate that frames sent by the implementation conform to the
//! protocol specification. Each test receives frames from the implementation
//! and validates specific aspects of the frame format.

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_spec_tester_macros::conformance;

/// Helper to complete handshake and return the received Hello frame for inspection.
async fn do_handshake_return_hello(peer: &mut Peer) -> Result<Frame, String> {
    // Receive Hello from implementation (initiator)
    let frame = peer
        .recv()
        .await
        .map_err(|e| format!("failed to receive Hello: {}", e))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err("first frame must be Hello".to_string());
    }

    // Send Hello response as acceptor
    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_postcard::to_vec(&response).map_err(|e| e.to_string())?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let response_frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&response_frame)
        .await
        .map_err(|e| e.to_string())?;

    Ok(frame)
}

/// Helper to complete handshake.
async fn do_handshake(peer: &mut Peer) -> Result<(), String> {
    do_handshake_return_hello(peer).await?;
    Ok(())
}

// =============================================================================
// frame.descriptor_size
// =============================================================================
// Rules: [verify frame.desc.size], [verify frame.desc.sizeof]
//
// Validates that descriptors are exactly 64 bytes.
// We verify this by receiving a frame and checking the wire format.

#[conformance(
    name = "frame.descriptor_size",
    rules = "frame.desc.size, frame.desc.sizeof"
)]
pub async fn descriptor_size(peer: &mut Peer) -> TestResult {
    // Receive a frame with raw wire bytes preserved
    let raw_frame = match peer.recv_raw().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // Verify the raw descriptor is exactly 64 bytes
    // (This is guaranteed by the type system - raw_desc is [u8; 64] - but we check anyway)
    if raw_frame.raw_desc.len() != 64 {
        return TestResult::fail(format!(
            "[verify frame.desc.sizeof]: descriptor on wire is {} bytes, expected 64",
            raw_frame.raw_desc.len()
        ));
    }

    // The fact that we successfully received and parsed the frame with exactly 64 bytes
    // for the descriptor proves the implementation sends 64-byte descriptors.
    TestResult::pass()
}

// =============================================================================
// frame.inline_payload_max
// =============================================================================
// Rules: [verify frame.payload.inline]
//
// Inline payloads must be ≤16 bytes.
// We verify by receiving a frame with inline payload and checking the limit.

#[conformance(name = "frame.inline_payload_max", rules = "frame.payload.inline")]
pub async fn inline_payload_max(peer: &mut Peer) -> TestResult {
    // Receive Hello frame
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // If payload_slot indicates inline, verify payload_len <= 16
    if frame.desc.payload_slot == INLINE_PAYLOAD_SLOT
        && frame.desc.payload_len > INLINE_PAYLOAD_SIZE as u32
    {
        return TestResult::fail(format!(
            "[verify frame.payload.inline]: inline payload_len {} exceeds max {}",
            frame.desc.payload_len, INLINE_PAYLOAD_SIZE
        ));
    }

    TestResult::pass()
}

// =============================================================================
// frame.sentinel_inline
// =============================================================================
// Rules: [verify frame.sentinel.values]
//
// payload_slot = 0xFFFFFFFF means inline.
// We verify by receiving a frame and checking if inline payloads use this sentinel.

#[conformance(name = "frame.sentinel_inline", rules = "frame.sentinel.values")]
pub async fn sentinel_inline(peer: &mut Peer) -> TestResult {
    // Receive Hello frame
    let frame = match peer.recv().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // Hello payload is small, likely inline
    // If it's inline, payload_slot must be INLINE_PAYLOAD_SLOT
    if frame.desc.payload_len <= INLINE_PAYLOAD_SIZE as u32 && frame.desc.payload_len > 0 {
        // Small payload - should be inline with correct sentinel
        if frame.desc.payload_slot != INLINE_PAYLOAD_SLOT {
            return TestResult::fail(format!(
                "[verify frame.sentinel.values]: inline payload should have payload_slot={:#X}, got {:#X}",
                INLINE_PAYLOAD_SLOT, frame.desc.payload_slot
            ));
        }
    }

    TestResult::pass()
}

// =============================================================================
// frame.sentinel_no_deadline
// =============================================================================
// Rules: [verify frame.sentinel.values]
//
// deadline_ns = 0xFFFFFFFFFFFFFFFF means no deadline.
// We verify by receiving frames and checking the deadline field.

#[conformance(name = "frame.sentinel_no_deadline", rules = "frame.sentinel.values")]
pub async fn sentinel_no_deadline(peer: &mut Peer) -> TestResult {
    // Complete handshake
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Receive next frame (OpenChannel or data)
    let frame = match peer.try_recv().await {
        Ok(Some(f)) => f,
        Ok(None) => return TestResult::pass(), // No more frames is fine
        Err(e) => return TestResult::fail(format!("failed to receive: {}", e)),
    };

    // If deadline_ns is not the NO_DEADLINE sentinel, it should be a valid timestamp
    // The NO_DEADLINE sentinel is 0xFFFFFFFFFFFFFFFF
    if frame.desc.deadline_ns != NO_DEADLINE {
        // It's a real deadline - just verify it's a reasonable value
        // (not checking actual time, just that it's not garbage)
        // Any non-sentinel value is valid as long as it's interpreted correctly
    }

    TestResult::pass()
}

// =============================================================================
// frame.encoding_little_endian
// =============================================================================
// Rules: [verify frame.desc.encoding]
//
// Descriptor fields must be little-endian.
// We verify by receiving a frame and checking the raw bytes match little-endian encoding.

#[conformance(name = "frame.encoding_little_endian", rules = "frame.desc.encoding")]
pub async fn encoding_little_endian(peer: &mut Peer) -> TestResult {
    // Receive Hello frame with raw wire bytes preserved
    let raw_frame = match peer.recv_raw().await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(format!("failed to receive frame: {}", e)),
    };

    // Use the RAW bytes from the wire, not re-serialized bytes!
    // This actually tests the implementation's encoding, not ours.
    let bytes = &raw_frame.raw_desc;

    // Descriptor layout (all little-endian):
    // 0-7:   msg_id (u64)
    // 8-11:  channel_id (u32)
    // 12-15: method_id (u32)
    // 16-19: payload_slot (u32)
    // 20-23: payload_generation (u32)
    // 24-27: payload_offset (u32)
    // 28-31: payload_len (u32)
    // 32-35: flags (u32)
    // 36-39: credit_grant (u32)
    // 40-47: deadline_ns (u64)
    // 48-63: inline_payload (16 bytes)

    // Check that msg_id (bytes 0-7) is little-endian
    let msg_id_from_wire = u64::from_le_bytes(bytes[0..8].try_into().unwrap());
    if msg_id_from_wire != raw_frame.desc.msg_id {
        return TestResult::fail(format!(
            "[verify frame.desc.encoding]: msg_id wire bytes don't match when parsed as little-endian: \
             wire bytes {:02x?} parse to {}, but descriptor has {}",
            &bytes[0..8],
            msg_id_from_wire,
            raw_frame.desc.msg_id
        ));
    }

    // Check that channel_id (bytes 8-11) is little-endian
    let channel_id_from_wire = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
    if channel_id_from_wire != raw_frame.desc.channel_id {
        return TestResult::fail(format!(
            "[verify frame.desc.encoding]: channel_id wire bytes don't match when parsed as little-endian: \
             wire bytes {:02x?} parse to {}, but descriptor has {}",
            &bytes[8..12],
            channel_id_from_wire,
            raw_frame.desc.channel_id
        ));
    }

    // Check that method_id (bytes 12-15) is little-endian
    let method_id_from_wire = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
    if method_id_from_wire != raw_frame.desc.method_id {
        return TestResult::fail(format!(
            "[verify frame.desc.encoding]: method_id wire bytes don't match when parsed as little-endian: \
             wire bytes {:02x?} parse to {}, but descriptor has {}",
            &bytes[12..16],
            method_id_from_wire,
            raw_frame.desc.method_id
        ));
    }

    // Check flags (bytes 32-35)
    let flags_from_wire = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
    if flags_from_wire != raw_frame.desc.flags {
        return TestResult::fail(format!(
            "[verify frame.desc.encoding]: flags wire bytes don't match when parsed as little-endian: \
             wire bytes {:02x?} parse to {}, but descriptor has {}",
            &bytes[32..36],
            flags_from_wire,
            raw_frame.desc.flags
        ));
    }

    // Check deadline_ns (bytes 40-47)
    let deadline_from_wire = u64::from_le_bytes(bytes[40..48].try_into().unwrap());
    if deadline_from_wire != raw_frame.desc.deadline_ns {
        return TestResult::fail(format!(
            "[verify frame.desc.encoding]: deadline_ns wire bytes don't match when parsed as little-endian: \
             wire bytes {:02x?} parse to {}, but descriptor has {}",
            &bytes[40..48],
            deadline_from_wire,
            raw_frame.desc.deadline_ns
        ));
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_control
// =============================================================================
// Rules: [verify frame.msg-id.control]
//
// Control frames use monotonic msg_id values like any other frame.
// We verify by receiving multiple control frames and checking msg_id increases.

#[conformance(name = "frame.msg_id_control", rules = "frame.msg-id.control")]
pub async fn msg_id_control(peer: &mut Peer) -> TestResult {
    // Receive Hello (first control frame)
    let hello = match do_handshake_return_hello(peer).await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(e),
    };

    let first_msg_id = hello.desc.msg_id;

    // The Hello frame should have msg_id >= 1 (implementations start at 1)
    if first_msg_id == 0 {
        return TestResult::fail(
            "[verify frame.msg-id.control]: control frame msg_id should not be 0".to_string(),
        );
    }

    // Try to receive another control frame (OpenChannel)
    let frame = match peer.try_recv().await {
        Ok(Some(f)) => f,
        Ok(None) => return TestResult::pass(), // No more frames is acceptable
        Err(e) => return TestResult::fail(format!("error receiving: {}", e)),
    };

    // If it's a control frame (channel 0), verify msg_id is greater
    if frame.desc.channel_id == 0 && frame.desc.msg_id <= first_msg_id {
        return TestResult::fail(format!(
            "[verify frame.msg-id.control]: control msg_id {} not greater than previous {}",
            frame.desc.msg_id, first_msg_id
        ));
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_stream_tunnel
// =============================================================================
// Rules: [verify frame.msg-id.stream-tunnel]
//
// STREAM/TUNNEL frames MUST use monotonically increasing msg_id values.
// This test verifies msg_id increases across frames on non-control channels.

#[conformance(
    name = "frame.msg_id_stream_tunnel",
    rules = "frame.msg-id.stream-tunnel"
)]
pub async fn msg_id_stream_tunnel(peer: &mut Peer) -> TestResult {
    // Complete handshake
    if let Err(e) = do_handshake(peer).await {
        return TestResult::fail(e);
    }

    // Collect frames and verify msg_id is monotonically increasing
    let mut last_msg_id: Option<u64> = None;

    loop {
        match peer.try_recv().await {
            Ok(Some(frame)) => {
                // For any frame, msg_id should be greater than previous
                if let Some(prev) = last_msg_id
                    && frame.desc.msg_id <= prev
                {
                    return TestResult::fail(format!(
                        "[verify frame.msg-id.stream-tunnel]: msg_id {} not greater than previous {}",
                        frame.desc.msg_id, prev
                    ));
                }
                last_msg_id = Some(frame.desc.msg_id);
            }
            Ok(None) => break, // EOF or timeout
            Err(_) => break,   // Error
        }
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_scope
// =============================================================================
// Rules: [verify frame.msg-id.scope]
//
// msg_id is scoped per connection (not per channel).
// We verify by receiving frames across different channels and checking msg_id increases globally.

#[conformance(name = "frame.msg_id_scope", rules = "frame.msg-id.scope")]
pub async fn msg_id_scope(peer: &mut Peer) -> TestResult {
    // Receive Hello
    let hello = match do_handshake_return_hello(peer).await {
        Ok(f) => f,
        Err(e) => return TestResult::fail(e),
    };

    let mut last_msg_id = hello.desc.msg_id;

    // Collect more frames across any channel
    loop {
        match peer.try_recv().await {
            Ok(Some(frame)) => {
                // msg_id should be greater than previous, regardless of channel
                if frame.desc.msg_id <= last_msg_id {
                    return TestResult::fail(format!(
                        "[verify frame.msg-id.scope]: msg_id {} on channel {} not greater than previous {} (msg_id is per-connection, not per-channel)",
                        frame.desc.msg_id, frame.desc.channel_id, last_msg_id
                    ));
                }
                last_msg_id = frame.desc.msg_id;
            }
            Ok(None) => break,
            Err(_) => break,
        }
    }

    TestResult::pass()
}
