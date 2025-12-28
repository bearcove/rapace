//! Protocol types matching the Rapace specification.
//!
//! These types define the wire format for the conformance test suite.
//! They are intentionally separate from rapace-core to ensure we're testing
//! against the spec, not against our own implementation.

use facet::Facet;

// =============================================================================
// Frame Format (spec: frame-format.md)
// =============================================================================

/// Sentinel value indicating payload is inline.
/// Spec: `r[frame.sentinel.values]`
pub const INLINE_PAYLOAD_SLOT: u32 = 0xFFFFFFFF;

/// Sentinel value indicating no deadline.
/// Spec: `r[frame.sentinel.values]`
pub const NO_DEADLINE: u64 = 0xFFFFFFFFFFFFFFFF;

/// Size of inline payload in bytes.
/// Spec: `r[frame.payload.inline]`
pub const INLINE_PAYLOAD_SIZE: usize = 16;

/// Hot-path message descriptor (64 bytes).
/// Spec: `r[frame.desc.size]`
#[derive(Clone, Copy, Debug, Default)]
#[repr(C, align(64))]
pub struct MsgDescHot {
    // Identity (16 bytes)
    pub msg_id: u64,
    pub channel_id: u32,
    pub method_id: u32,

    // Payload location (16 bytes)
    pub payload_slot: u32,
    pub payload_generation: u32,
    pub payload_offset: u32,
    pub payload_len: u32,

    // Flow control & timing (16 bytes)
    pub flags: u32,
    pub credit_grant: u32,
    pub deadline_ns: u64,

    // Inline payload (16 bytes)
    pub inline_payload: [u8; INLINE_PAYLOAD_SIZE],
}

const _: () = assert!(core::mem::size_of::<MsgDescHot>() == 64);

impl MsgDescHot {
    pub fn new() -> Self {
        Self {
            payload_slot: INLINE_PAYLOAD_SLOT,
            deadline_ns: NO_DEADLINE,
            ..Default::default()
        }
    }

    /// Encode descriptor to bytes (little-endian).
    /// Spec: `r[frame.desc.encoding]`
    pub fn to_bytes(&self) -> [u8; 64] {
        let mut buf = [0u8; 64];
        buf[0..8].copy_from_slice(&self.msg_id.to_le_bytes());
        buf[8..12].copy_from_slice(&self.channel_id.to_le_bytes());
        buf[12..16].copy_from_slice(&self.method_id.to_le_bytes());
        buf[16..20].copy_from_slice(&self.payload_slot.to_le_bytes());
        buf[20..24].copy_from_slice(&self.payload_generation.to_le_bytes());
        buf[24..28].copy_from_slice(&self.payload_offset.to_le_bytes());
        buf[28..32].copy_from_slice(&self.payload_len.to_le_bytes());
        buf[32..36].copy_from_slice(&self.flags.to_le_bytes());
        buf[36..40].copy_from_slice(&self.credit_grant.to_le_bytes());
        buf[40..48].copy_from_slice(&self.deadline_ns.to_le_bytes());
        buf[48..64].copy_from_slice(&self.inline_payload);
        buf
    }

    /// Decode descriptor from bytes (little-endian).
    /// Spec: `r[frame.desc.encoding]`
    pub fn from_bytes(buf: &[u8; 64]) -> Self {
        Self {
            msg_id: u64::from_le_bytes(buf[0..8].try_into().unwrap()),
            channel_id: u32::from_le_bytes(buf[8..12].try_into().unwrap()),
            method_id: u32::from_le_bytes(buf[12..16].try_into().unwrap()),
            payload_slot: u32::from_le_bytes(buf[16..20].try_into().unwrap()),
            payload_generation: u32::from_le_bytes(buf[20..24].try_into().unwrap()),
            payload_offset: u32::from_le_bytes(buf[24..28].try_into().unwrap()),
            payload_len: u32::from_le_bytes(buf[28..32].try_into().unwrap()),
            flags: u32::from_le_bytes(buf[32..36].try_into().unwrap()),
            credit_grant: u32::from_le_bytes(buf[36..40].try_into().unwrap()),
            deadline_ns: u64::from_le_bytes(buf[40..48].try_into().unwrap()),
            inline_payload: buf[48..64].try_into().unwrap(),
        }
    }

    pub fn is_inline(&self) -> bool {
        self.payload_slot == INLINE_PAYLOAD_SLOT
    }
}

// =============================================================================
// Frame Flags (spec: core.md#frameflags)
// =============================================================================

pub mod flags {
    /// Frame carries payload data.
    pub const DATA: u32 = 0b0000_0001;
    /// Control message (channel 0 only).
    /// Spec: `r[core.control.flag-set]`, `r[core.control.flag-clear]`
    pub const CONTROL: u32 = 0b0000_0010;
    /// End of stream (half-close).
    /// Spec: `r[core.eos.after-send]`
    pub const EOS: u32 = 0b0000_0100;
    /// Reserved (do not use).
    pub const _RESERVED_08: u32 = 0b0000_1000;
    /// Error response.
    /// Spec: `r[core.call.error.flags]`
    pub const ERROR: u32 = 0b0001_0000;
    /// Priority hint (maps to priority 192).
    /// Spec: `r[core.flags.high-priority]`
    pub const HIGH_PRIORITY: u32 = 0b0010_0000;
    /// Contains credit grant.
    /// Spec: `r[core.flow.credit-semantics]`
    pub const CREDITS: u32 = 0b0100_0000;
    /// Reserved (do not use).
    pub const _RESERVED_80: u32 = 0b1000_0000;
    /// Fire-and-forget (no response expected).
    pub const NO_REPLY: u32 = 0b0001_0000_0000;
    /// This is a response frame.
    /// Spec: `r[core.call.response.flags]`
    pub const RESPONSE: u32 = 0b0010_0000_0000;
}

// =============================================================================
// Control Verbs (spec: core.md#control-verbs)
// =============================================================================

pub mod control_verb {
    /// Hello (handshake).
    pub const HELLO: u32 = 0;
    /// Open a new channel.
    pub const OPEN_CHANNEL: u32 = 1;
    /// Close a channel gracefully.
    pub const CLOSE_CHANNEL: u32 = 2;
    /// Cancel a channel (abort).
    pub const CANCEL_CHANNEL: u32 = 3;
    /// Grant flow control credits.
    pub const GRANT_CREDITS: u32 = 4;
    /// Liveness probe.
    pub const PING: u32 = 5;
    /// Response to Ping.
    pub const PONG: u32 = 6;
    /// Graceful shutdown.
    pub const GO_AWAY: u32 = 7;
}

// =============================================================================
// Handshake Types (spec: handshake.md)
// =============================================================================

/// Protocol version packed as (major << 16) | minor.
/// For v1.0: 0x00010000
pub const PROTOCOL_VERSION_1_0: u32 = 0x00010000;

/// Connection role.
/// Spec: `r[handshake.role.validation]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Role {
    Initiator = 1,
    Acceptor = 2,
}

/// Feature bits for capability negotiation.
/// Spec: `r[handshake.features.required]`
pub mod features {
    /// Supports STREAM/TUNNEL channels attached to calls.
    pub const ATTACHED_STREAMS: u64 = 1 << 0;
    /// Uses CallResult envelope with status + trailers.
    pub const CALL_ENVELOPE: u64 = 1 << 1;
    /// Enforces credit-based flow control.
    pub const CREDIT_FLOW_CONTROL: u64 = 1 << 2;
    /// Supports Rapace-level Ping/Pong.
    pub const RAPACE_PING: u64 = 1 << 3;
    /// WebTransport: map channels to QUIC streams.
    pub const WEBTRANSPORT_MULTI_STREAM: u64 = 1 << 4;
    /// WebTransport: support unreliable datagrams.
    pub const WEBTRANSPORT_DATAGRAMS: u64 = 1 << 5;
}

/// Connection limits.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Limits {
    /// Maximum payload bytes per frame.
    pub max_payload_size: u32,
    /// Maximum concurrent channels (0 = unlimited).
    pub max_channels: u32,
    /// Maximum pending RPC calls (0 = unlimited).
    pub max_pending_calls: u32,
}

impl Default for Limits {
    fn default() -> Self {
        Self {
            max_payload_size: 1024 * 1024, // 1MB
            max_channels: 0,               // unlimited
            max_pending_calls: 0,          // unlimited
        }
    }
}

/// Method registry entry.
/// Spec: `r[handshake.registry.validation]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct MethodInfo {
    /// Method identifier (FNV-1a hash).
    /// Spec: `r[core.method-id.algorithm]`
    pub method_id: u32,
    /// Structural signature hash (BLAKE3).
    /// Spec: `r[handshake.sig-hash.blake3]`
    pub sig_hash: [u8; 32],
    /// Human-readable name for debugging.
    pub name: Option<String>,
}

/// Hello message for handshake.
/// Spec: `r[handshake.required]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Hello {
    /// Protocol version.
    /// Spec: `r[handshake.version.major]`, `r[handshake.version.minor]`
    pub protocol_version: u32,
    /// Connection role.
    /// Spec: `r[handshake.role.validation]`
    pub role: Role,
    /// Features the peer MUST support.
    /// Spec: `r[handshake.features.required]`
    pub required_features: u64,
    /// Features the peer supports.
    pub supported_features: u64,
    /// Advertised limits.
    pub limits: Limits,
    /// Method registry.
    /// Spec: `r[handshake.registry.validation]`
    pub methods: Vec<MethodInfo>,
    /// Extension parameters.
    /// Spec: `r[handshake.params.unknown]`
    pub params: Vec<(String, Vec<u8>)>,
}

impl Default for Hello {
    fn default() -> Self {
        Self {
            protocol_version: PROTOCOL_VERSION_1_0,
            role: Role::Initiator,
            required_features: features::ATTACHED_STREAMS | features::CALL_ENVELOPE,
            supported_features: features::ATTACHED_STREAMS
                | features::CALL_ENVELOPE
                | features::CREDIT_FLOW_CONTROL
                | features::RAPACE_PING,
            limits: Limits::default(),
            methods: Vec::new(),
            params: Vec::new(),
        }
    }
}

// =============================================================================
// Channel Types (spec: core.md#channel-kinds)
// =============================================================================

/// Channel kind.
/// Spec: `r[core.channel.kind]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum ChannelKind {
    Call = 1,
    Stream = 2,
    Tunnel = 3,
}

/// Stream/tunnel direction.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum Direction {
    ClientToServer = 1,
    ServerToClient = 2,
    Bidir = 3,
}

/// Attachment info for STREAM/TUNNEL channels.
/// Spec: `r[core.channel.open.attach-required]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct AttachTo {
    /// The parent CALL channel.
    pub call_channel_id: u32,
    /// Port identifier from method signature.
    /// Spec: `r[core.stream.port-id-assignment]`
    pub port_id: u32,
    /// Direction.
    pub direction: Direction,
}

/// OpenChannel control message.
/// Spec: `r[core.channel.open]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct OpenChannel {
    /// The new channel's ID.
    /// Spec: `r[core.channel.id.allocation]`
    pub channel_id: u32,
    /// Channel kind.
    /// Spec: `r[core.channel.kind]`
    pub kind: ChannelKind,
    /// Attachment for STREAM/TUNNEL.
    /// Spec: `r[core.channel.open.attach-required]`
    pub attach: Option<AttachTo>,
    /// Metadata (tracing, auth, etc.).
    pub metadata: Vec<(String, Vec<u8>)>,
    /// Initial flow control credits.
    pub initial_credits: u32,
}

// =============================================================================
// Close/Cancel Types (spec: core.md#half-close-and-termination)
// =============================================================================

/// Reason for closing a channel.
/// Spec: `r[core.close.close-channel-semantics]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CloseReason {
    Normal,
    Error(String),
}

/// Reason for canceling a channel.
/// Spec: `r[core.cancel.behavior]`
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum CancelReason {
    ClientCancel = 1,
    DeadlineExceeded = 2,
    ResourceExhausted = 3,
    ProtocolViolation = 4,
    Unauthenticated = 5,
    PermissionDenied = 6,
}

/// CloseChannel control message.
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct CloseChannel {
    pub channel_id: u32,
    pub reason: CloseReason,
}

/// CancelChannel control message.
/// Spec: `r[core.cancel.idempotent]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct CancelChannel {
    pub channel_id: u32,
    pub reason: CancelReason,
}

// =============================================================================
// Flow Control (spec: core.md#flow-control)
// =============================================================================

/// GrantCredits control message.
/// Spec: `r[core.flow.credit-additive]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct GrantCredits {
    pub channel_id: u32,
    pub bytes: u32,
}

// =============================================================================
// Ping/Pong (spec: core.md#pingpong)
// =============================================================================

/// Ping control message.
/// Spec: `r[core.ping.semantics]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Ping {
    pub payload: [u8; 8],
}

/// Pong control message.
/// Spec: `r[core.ping.semantics]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Pong {
    pub payload: [u8; 8],
}

// =============================================================================
// GoAway (spec: core.md#goaway)
// =============================================================================

/// Reason for GoAway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Facet)]
#[repr(u8)]
pub enum GoAwayReason {
    Shutdown = 1,
    Maintenance = 2,
    Overload = 3,
    ProtocolError = 4,
}

/// GoAway control message.
/// Spec: `r[core.goaway.last-channel-id]`, `r[core.goaway.after-send]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct GoAway {
    pub reason: GoAwayReason,
    /// Last channel ID the sender will process.
    pub last_channel_id: u32,
    /// Human-readable reason.
    pub message: String,
    /// Extension data.
    pub metadata: Vec<(String, Vec<u8>)>,
}

// =============================================================================
// Error Types (spec: errors.md)
// =============================================================================

/// RPC status.
/// Spec: `r[error.status.success]`, `r[error.status.error]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct Status {
    /// Error code (0 = OK).
    pub code: u32,
    /// Human-readable description.
    pub message: String,
    /// Opaque structured details.
    pub details: Vec<u8>,
}

impl Status {
    pub fn ok() -> Self {
        Self {
            code: 0,
            message: String::new(),
            details: Vec::new(),
        }
    }

    pub fn error(code: u32, message: impl Into<String>) -> Self {
        Self {
            code,
            message: message.into(),
            details: Vec::new(),
        }
    }
}

/// CallResult envelope.
/// Spec: `r[core.call.result.envelope]`
#[derive(Debug, Clone, PartialEq, Eq, Facet)]
pub struct CallResult {
    pub status: Status,
    pub trailers: Vec<(String, Vec<u8>)>,
    /// Present on success, absent on error.
    pub body: Option<Vec<u8>>,
}

/// Standard error codes (gRPC-compatible).
/// Spec: errors.md
pub mod error_code {
    pub const OK: u32 = 0;
    pub const CANCELLED: u32 = 1;
    pub const UNKNOWN: u32 = 2;
    pub const INVALID_ARGUMENT: u32 = 3;
    pub const DEADLINE_EXCEEDED: u32 = 4;
    pub const NOT_FOUND: u32 = 5;
    pub const ALREADY_EXISTS: u32 = 6;
    pub const PERMISSION_DENIED: u32 = 7;
    pub const RESOURCE_EXHAUSTED: u32 = 8;
    pub const FAILED_PRECONDITION: u32 = 9;
    pub const ABORTED: u32 = 10;
    pub const OUT_OF_RANGE: u32 = 11;
    pub const UNIMPLEMENTED: u32 = 12;
    pub const INTERNAL: u32 = 13;
    pub const UNAVAILABLE: u32 = 14;
    pub const DATA_LOSS: u32 = 15;
    pub const UNAUTHENTICATED: u32 = 16;
    pub const INCOMPATIBLE_SCHEMA: u32 = 17;

    // Protocol error codes (50-99)
    pub const PROTOCOL_ERROR: u32 = 50;
    pub const INVALID_FRAME: u32 = 51;
    pub const INVALID_CHANNEL: u32 = 52;
    pub const INVALID_METHOD: u32 = 53;
    pub const DECODE_ERROR: u32 = 54;
    pub const ENCODE_ERROR: u32 = 55;
}

// =============================================================================
// Method ID Computation (spec: core.md#method-id-computation)
// =============================================================================

/// Compute method ID using FNV-1a hash.
/// Spec: `r[core.method-id.algorithm]`
pub fn compute_method_id(service_name: &str, method_name: &str) -> u32 {
    const FNV_OFFSET: u64 = 0xcbf29ce484222325;
    const FNV_PRIME: u64 = 0x100000001b3;

    let mut hash: u64 = FNV_OFFSET;

    // Hash service name
    for byte in service_name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Hash separator
    hash ^= b'.' as u64;
    hash = hash.wrapping_mul(FNV_PRIME);

    // Hash method name
    for byte in method_name.bytes() {
        hash ^= byte as u64;
        hash = hash.wrapping_mul(FNV_PRIME);
    }

    // Fold to 32 bits
    ((hash >> 32) ^ hash) as u32
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_msg_desc_hot_size() {
        assert_eq!(core::mem::size_of::<MsgDescHot>(), 64);
    }

    #[test]
    fn test_msg_desc_hot_roundtrip() {
        let mut desc = MsgDescHot::new();
        desc.msg_id = 12345;
        desc.channel_id = 7;
        desc.method_id = 0xDEADBEEF;
        desc.flags = flags::DATA | flags::EOS;
        desc.inline_payload[0..4].copy_from_slice(b"test");
        desc.payload_len = 4;

        let bytes = desc.to_bytes();
        let decoded = MsgDescHot::from_bytes(&bytes);

        assert_eq!(decoded.msg_id, 12345);
        assert_eq!(decoded.channel_id, 7);
        assert_eq!(decoded.method_id, 0xDEADBEEF);
        assert_eq!(decoded.flags, flags::DATA | flags::EOS);
        assert_eq!(&decoded.inline_payload[0..4], b"test");
    }

    #[test]
    fn test_method_id_computation() {
        // Test that the same input produces consistent output
        let id1 = compute_method_id("Calculator", "add");
        let id2 = compute_method_id("Calculator", "add");
        assert_eq!(id1, id2);

        // Different methods should produce different IDs
        let id3 = compute_method_id("Calculator", "subtract");
        assert_ne!(id1, id3);
    }
}
