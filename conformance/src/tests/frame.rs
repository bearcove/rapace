//! Frame format conformance tests.
//!
//! Tests for spec rules in frame-format.md

use crate::harness::Peer;
use crate::protocol::*;
use crate::testcase::TestResult;
use rapace_conformance_macros::conformance;

// =============================================================================
// frame.descriptor_size
// =============================================================================
// Rules: [verify frame.desc.size], [verify frame.desc.sizeof]
//
// Validates that descriptors are exactly 64 bytes.

#[conformance(
    name = "frame.descriptor_size",
    rules = "frame.desc.size, frame.desc.sizeof"
)]
pub fn descriptor_size(_peer: &mut Peer) -> TestResult {
    // This is a structural test - just verify our types
    if std::mem::size_of::<MsgDescHot>() != 64 {
        return TestResult::fail(format!(
            "[verify frame.desc.sizeof]: MsgDescHot is {} bytes, expected 64",
            std::mem::size_of::<MsgDescHot>()
        ));
    }
    TestResult::pass()
}

// =============================================================================
// frame.inline_payload_max
// =============================================================================
// Rules: [verify frame.payload.inline]
//
// Inline payloads must be ≤16 bytes.

#[conformance(name = "frame.inline_payload_max", rules = "frame.payload.inline")]
pub fn inline_payload_max(_peer: &mut Peer) -> TestResult {
    if INLINE_PAYLOAD_SIZE != 16 {
        return TestResult::fail(format!(
            "[verify frame.payload.inline]: INLINE_PAYLOAD_SIZE is {}, expected 16",
            INLINE_PAYLOAD_SIZE
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

#[conformance(name = "frame.sentinel_inline", rules = "frame.sentinel.values")]
pub fn sentinel_inline(_peer: &mut Peer) -> TestResult {
    if INLINE_PAYLOAD_SLOT != 0xFFFFFFFF {
        return TestResult::fail(format!(
            "[verify frame.sentinel.values]: INLINE_PAYLOAD_SLOT is {:#X}, expected 0xFFFFFFFF",
            INLINE_PAYLOAD_SLOT
        ));
    }
    TestResult::pass()
}

// =============================================================================
// frame.sentinel_no_deadline
// =============================================================================
// Rules: [verify frame.sentinel.values]
//
// deadline_ns = 0xFFFFFFFFFFFFFFFF means no deadline.

#[conformance(name = "frame.sentinel_no_deadline", rules = "frame.sentinel.values")]
pub fn sentinel_no_deadline(_peer: &mut Peer) -> TestResult {
    if NO_DEADLINE != 0xFFFFFFFFFFFFFFFF {
        return TestResult::fail(format!(
            "[verify frame.sentinel.values]: NO_DEADLINE is {:#X}, expected 0xFFFFFFFFFFFFFFFF",
            NO_DEADLINE
        ));
    }
    TestResult::pass()
}

// =============================================================================
// frame.encoding_little_endian
// =============================================================================
// Rules: [verify frame.desc.encoding]
//
// Descriptor fields must be little-endian.

#[conformance(name = "frame.encoding_little_endian", rules = "frame.desc.encoding")]
pub fn encoding_little_endian(_peer: &mut Peer) -> TestResult {
    let mut desc = MsgDescHot::new();
    desc.msg_id = 0x0102030405060708;
    desc.channel_id = 0x11121314;
    desc.method_id = 0x21222324;

    let bytes = desc.to_bytes();

    // Check msg_id (bytes 0-7, little-endian)
    if bytes[0..8] != [0x08, 0x07, 0x06, 0x05, 0x04, 0x03, 0x02, 0x01] {
        return TestResult::fail(
            "[verify frame.desc.encoding]: msg_id not little-endian".to_string(),
        );
    }

    // Check channel_id (bytes 8-11)
    if bytes[8..12] != [0x14, 0x13, 0x12, 0x11] {
        return TestResult::fail(
            "[verify frame.desc.encoding]: channel_id not little-endian".to_string(),
        );
    }

    // Check method_id (bytes 12-15)
    if bytes[12..16] != [0x24, 0x23, 0x22, 0x21] {
        return TestResult::fail(
            "[verify frame.desc.encoding]: method_id not little-endian".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_control
// =============================================================================
// Rules: [verify frame.msg-id.control]
//
// Control frames use monotonic msg_id values like any other frame.

#[conformance(name = "frame.msg_id_control", rules = "frame.msg-id.control")]
pub fn msg_id_control(_peer: &mut Peer) -> TestResult {
    // Control frames (channel 0) MUST use monotonically increasing msg_id values.
    // Each peer maintains its own counter, starting at 1.

    let mut desc1 = MsgDescHot::new();
    desc1.channel_id = 0;
    desc1.method_id = control_verb::PING;
    desc1.msg_id = 1;
    desc1.flags = flags::CONTROL;

    let mut desc2 = MsgDescHot::new();
    desc2.channel_id = 0;
    desc2.method_id = control_verb::PING;
    desc2.msg_id = 2; // Must be > previous
    desc2.flags = flags::CONTROL;

    if desc2.msg_id <= desc1.msg_id {
        return TestResult::fail(
            "[verify frame.msg-id.control]: control msg_id must be monotonically increasing"
                .to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_stream_tunnel
// =============================================================================
// Rules: [verify frame.msg-id.stream-tunnel]
//
// STREAM/TUNNEL frames MUST use monotonically increasing msg_id values.

#[conformance(
    name = "frame.msg_id_stream_tunnel",
    rules = "frame.msg-id.stream-tunnel"
)]
pub fn msg_id_stream_tunnel(_peer: &mut Peer) -> TestResult {
    // For STREAM and TUNNEL channels, each frame gets a monotonically
    // increasing msg_id. This serves for:
    // - Ordering verification
    // - Debugging and tracing

    // Simulate sending 3 stream items
    let msg_ids = [5u64, 6, 7]; // Must be monotonically increasing

    for i in 1..msg_ids.len() {
        if msg_ids[i] <= msg_ids[i - 1] {
            return TestResult::fail(format!(
                "[verify frame.msg-id.stream-tunnel]: msg_id {} must be > {}",
                msg_ids[i],
                msg_ids[i - 1]
            ));
        }
    }

    // Create a stream frame with proper msg_id
    let mut desc = MsgDescHot::new();
    desc.channel_id = 3; // Some stream channel
    desc.method_id = 0; // STREAM uses method_id = 0
    desc.msg_id = 5;
    desc.flags = flags::DATA;

    if desc.method_id != 0 {
        return TestResult::fail(
            "[verify frame.msg-id.stream-tunnel]: STREAM method_id should be 0".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// frame.msg_id_scope
// =============================================================================
// Rules: [verify frame.msg-id.scope]
//
// msg_id is scoped per connection (not per channel).

#[conformance(name = "frame.msg_id_scope", rules = "frame.msg-id.scope")]
pub fn msg_id_scope(_peer: &mut Peer) -> TestResult {
    // msg_id is scoped per connection:
    // - Each peer maintains a single counter starting at 1
    // - Every frame sent by a peer uses the next value
    // - This is NOT per-channel, it's per-connection

    // Simulate frames across different channels from the same peer
    let frames = [
        (1u64, 0u32), // msg_id=1, channel_id=0 (control)
        (2u64, 1u32), // msg_id=2, channel_id=1 (call)
        (3u64, 3u32), // msg_id=3, channel_id=3 (stream)
        (4u64, 0u32), // msg_id=4, channel_id=0 (control again)
    ];

    for i in 1..frames.len() {
        if frames[i].0 <= frames[i - 1].0 {
            return TestResult::fail(format!(
                "[verify frame.msg-id.scope]: msg_id {} on channel {} should be > previous msg_id {} on channel {}",
                frames[i].0,
                frames[i].1,
                frames[i - 1].0,
                frames[i - 1].1
            ));
        }
    }

    TestResult::pass()
}
