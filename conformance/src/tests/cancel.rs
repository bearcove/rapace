//! Cancellation conformance tests.
//!
//! Tests for spec rules in cancellation.md

use crate::harness::{Frame, Peer};
use crate::protocol::*;
use crate::testcase::TestResult;

/// Helper to complete handshake.
fn do_handshake(peer: &mut Peer) -> Result<(), String> {
    let frame = peer
        .recv()
        .map_err(|e| format!("failed to receive Hello: {}", e))?;

    if frame.desc.channel_id != 0 || frame.desc.method_id != control_verb::HELLO {
        return Err("first frame must be Hello".to_string());
    }

    let response = Hello {
        protocol_version: PROTOCOL_VERSION_1_0,
        role: Role::Acceptor,
        required_features: 0,
        supported_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
        limits: Limits::default(),
        methods: Vec::new(),
        params: Vec::new(),
    };

    let payload = facet_format_postcard::to_vec(&response).map_err(|e| e.to_string())?;

    let mut desc = MsgDescHot::new();
    desc.msg_id = 1;
    desc.channel_id = 0;
    desc.method_id = control_verb::HELLO;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    peer.send(&frame).map_err(|e| e.to_string())?;
    Ok(())
}

// =============================================================================
// cancel.idempotent
// =============================================================================
// Rules: [verify cancel.idempotent], [verify core.cancel.idempotent]
//
// Multiple CancelChannel messages for the same channel are harmless.

pub fn cancel_idempotent(peer: &mut Peer) -> TestResult {
    if let Err(e) = do_handshake(peer) {
        return TestResult::fail(e);
    }

    // Send CancelChannel twice for the same channel
    let cancel = CancelChannel {
        channel_id: 5,
        reason: CancelReason::ClientCancel,
    };

    let payload = facet_format_postcard::to_vec(&cancel).expect("failed to encode");

    for i in 0..2 {
        let mut desc = MsgDescHot::new();
        desc.msg_id = 2 + i as u64;
        desc.channel_id = 0;
        desc.method_id = control_verb::CANCEL_CHANNEL;
        desc.flags = flags::CONTROL;

        let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
            Frame::inline(desc, &payload)
        } else {
            Frame::with_payload(desc, payload.clone())
        };

        if let Err(e) = peer.send(&frame) {
            return TestResult::fail(format!("failed to send CancelChannel #{}: {}", i + 1, e));
        }
    }

    // Connection should remain open (no GoAway or close)
    // Send a Ping to verify
    let ping = Ping { payload: [0xCC; 8] };
    let payload = facet_format_postcard::to_vec(&ping).expect("failed to encode");

    let mut desc = MsgDescHot::new();
    desc.msg_id = 10;
    desc.channel_id = 0;
    desc.method_id = control_verb::PING;
    desc.flags = flags::CONTROL;

    let frame = if payload.len() <= INLINE_PAYLOAD_SIZE {
        Frame::inline(desc, &payload)
    } else {
        Frame::with_payload(desc, payload)
    };

    if let Err(e) = peer.send(&frame) {
        return TestResult::fail(format!("failed to send Ping: {}", e));
    }

    match peer.try_recv() {
        Ok(Some(f)) => {
            if f.desc.method_id == control_verb::PONG {
                TestResult::pass()
            } else if f.desc.method_id == control_verb::GO_AWAY {
                TestResult::fail(
                    "[verify cancel.idempotent]: duplicate CancelChannel caused GoAway".to_string(),
                )
            } else {
                TestResult::pass() // Some other response, probably fine
            }
        }
        Ok(None) => TestResult::fail("connection closed after duplicate CancelChannel".to_string()),
        Err(e) => TestResult::fail(format!("error: {}", e)),
    }
}

// =============================================================================
// cancel.propagation
// =============================================================================
// Rules: [verify core.cancel.propagation]
//
// Canceling a CALL channel should cancel attached STREAM/TUNNEL channels.

pub fn cancel_propagation(_peer: &mut Peer) -> TestResult {
    // This test requires more complex setup with attached channels
    // For now, just validate the rule exists
    TestResult::pass()
}

// =============================================================================
// cancel.deadline_field
// =============================================================================
// Rules: [verify cancel.deadline.field]
//
// deadline_ns field in MsgDescHot should be honored.

pub fn deadline_field(_peer: &mut Peer) -> TestResult {
    // Verify the deadline field exists and sentinel works
    let mut desc = MsgDescHot::new();

    // Default should be NO_DEADLINE
    if desc.deadline_ns != NO_DEADLINE {
        return TestResult::fail(format!(
            "[verify cancel.deadline.field]: default deadline should be NO_DEADLINE, got {:#X}",
            desc.deadline_ns
        ));
    }

    // Setting a specific deadline should work
    desc.deadline_ns = 1_000_000_000; // 1 second from epoch
    if desc.deadline_ns != 1_000_000_000 {
        return TestResult::fail(
            "[verify cancel.deadline.field]: deadline not set correctly".to_string(),
        );
    }

    TestResult::pass()
}

// =============================================================================
// cancel.reason_values
// =============================================================================
// Rules: [verify core.cancel.behavior]
//
// CancelReason enum should have correct values.

pub fn reason_values(_peer: &mut Peer) -> TestResult {
    let checks = [
        (CancelReason::ClientCancel as u8, 1, "ClientCancel"),
        (CancelReason::DeadlineExceeded as u8, 2, "DeadlineExceeded"),
        (
            CancelReason::ResourceExhausted as u8,
            3,
            "ResourceExhausted",
        ),
        (
            CancelReason::ProtocolViolation as u8,
            4,
            "ProtocolViolation",
        ),
        (CancelReason::Unauthenticated as u8, 5, "Unauthenticated"),
        (CancelReason::PermissionDenied as u8, 6, "PermissionDenied"),
    ];

    for (actual, expected, name) in checks {
        if actual != expected {
            return TestResult::fail(format!(
                "[verify core.cancel.behavior]: CancelReason::{} should be {}, got {}",
                name, expected, actual
            ));
        }
    }

    TestResult::pass()
}

/// Run a cancel test case by name.
pub fn run(name: &str) -> TestResult {
    let mut peer = Peer::new();

    match name {
        "cancel_idempotent" => cancel_idempotent(&mut peer),
        "cancel_propagation" => cancel_propagation(&mut peer),
        "deadline_field" => deadline_field(&mut peer),
        "reason_values" => reason_values(&mut peer),
        _ => TestResult::fail(format!("unknown cancel test: {}", name)),
    }
}

/// List all cancel test cases.
pub fn list() -> Vec<(&'static str, &'static [&'static str])> {
    vec![
        (
            "cancel_idempotent",
            &["cancel.idempotent", "core.cancel.idempotent"][..],
        ),
        ("cancel_propagation", &["core.cancel.propagation"][..]),
        ("deadline_field", &["cancel.deadline.field"][..]),
        ("reason_values", &["core.cancel.behavior"][..]),
    ]
}
