//! SHM memory layout definitions.
//!
//! This module defines the `repr(C)` structures that make up the shared memory
//! segment. These are the canonical layouts; see `docs/content/guide/design.md`.
//!
//! # Memory Layout
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────┐
//! │  Segment Header (64 bytes, cache-line aligned)                       │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  A→B Descriptor Ring                                                 │
//! │    - Ring header (192 bytes: visible_head, tail, capacity + padding) │
//! │    - Descriptors (capacity × 64 bytes)                               │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  B→A Descriptor Ring                                                 │
//! │    - Ring header (192 bytes)                                         │
//! │    - Descriptors (capacity × 64 bytes)                               │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  Data Segment Header (64 bytes)                                      │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  Slot Metadata Array (slot_count × 8 bytes)                          │
//! ├─────────────────────────────────────────────────────────────────────┤
//! │  Slot Data (slot_count × slot_size bytes)                            │
//! └─────────────────────────────────────────────────────────────────────┘
//! ```

use std::sync::atomic::{AtomicU32, AtomicU64, Ordering};

use crate::MsgDescHot;

// Re-export types from shm-primitives for API compatibility
pub use shm_primitives::SpscRingHeader as DescRingHeader;
pub use shm_primitives::TreiberSlabHeader as DataSegmentHeader;
use shm_primitives::{AllocResult, SlotHandle, SpscRingRaw, TreiberSlabRaw};
pub use shm_primitives::{SlotMeta, SlotState};

/// Sentinel value indicating end of free list.
pub use shm_primitives::treiber::FREE_LIST_END;

/// Unpack a free list head into (index, tag).
#[inline]
fn unpack_free_head(packed: u64) -> (u32, u32) {
    let index = packed as u32;
    let tag = (packed >> 32) as u32;
    (index, tag)
}

/// Magic bytes identifying a rapace SHM segment.
pub const MAGIC: [u8; 8] = *b"RAPACE\0\0";

/// Current protocol version (major.minor packed into u32).
/// Major = high 16 bits, minor = low 16 bits.
pub const PROTOCOL_VERSION: u32 = 1 << 16; // v1.0

/// Default descriptor ring capacity (power of 2).
pub const DEFAULT_RING_CAPACITY: u32 = 256;

/// Default slot size in bytes (4KB).
pub const DEFAULT_SLOT_SIZE: u32 = 4096;

/// Default number of slots.
pub const DEFAULT_SLOT_COUNT: u32 = 64;

// =============================================================================
// Segment Header
// =============================================================================

/// Segment header at the start of the SHM region (128 bytes).
///
/// Contains version info, feature flags, configuration, peer liveness tracking,
/// and futex words for signaling.
#[repr(C, align(64))]
pub struct SegmentHeader {
    /// Magic bytes: "RAPACE\0\0".
    pub magic: [u8; 8],
    /// Protocol version (major.minor packed).
    pub version: u32,
    /// Feature flags.
    pub flags: u32,

    // Configuration (so opener can discover it from the file)
    /// Descriptor ring capacity (power of 2).
    pub ring_capacity: u32,
    /// Size of each data slot in bytes.
    pub slot_size: u32,
    /// Number of data slots.
    pub slot_count: u32,
    /// Reserved for future config fields.
    pub _config_reserved: u32,

    // Peer liveness (for crash detection)
    /// Incremented by peer A periodically.
    pub peer_a_epoch: AtomicU64,
    /// Incremented by peer B periodically.
    pub peer_b_epoch: AtomicU64,
    /// Timestamp of last peer A heartbeat (nanos since epoch).
    pub peer_a_last_seen: AtomicU64,
    /// Timestamp of last peer B heartbeat (nanos since epoch).
    pub peer_b_last_seen: AtomicU64,

    // Futex words for cross-process signaling (16 bytes)
    /// A signals after enqueue to A→B ring, B waits when ring empty.
    pub a_to_b_data_futex: AtomicU32,
    /// B signals after dequeue from A→B ring, A waits when ring full.
    pub a_to_b_space_futex: AtomicU32,
    /// B signals after enqueue to B→A ring, A waits when ring empty.
    pub b_to_a_data_futex: AtomicU32,
    /// A signals after dequeue from B→A ring, B waits when ring full.
    pub b_to_a_space_futex: AtomicU32,

    /// Padding to 128 bytes.
    pub _pad: [u8; 48],
}

const _: () = assert!(core::mem::size_of::<SegmentHeader>() == 128);

impl SegmentHeader {
    /// Initialize a new segment header with the given configuration.
    pub fn init(&mut self, ring_capacity: u32, slot_size: u32, slot_count: u32) {
        self.magic = MAGIC;
        self.version = PROTOCOL_VERSION;
        self.flags = 0;
        self.ring_capacity = ring_capacity;
        self.slot_size = slot_size;
        self.slot_count = slot_count;
        self._config_reserved = 0;
        self.peer_a_epoch = AtomicU64::new(0);
        self.peer_b_epoch = AtomicU64::new(0);
        self.peer_a_last_seen = AtomicU64::new(0);
        self.peer_b_last_seen = AtomicU64::new(0);
        // Initialize futex words to 0
        self.a_to_b_data_futex = AtomicU32::new(0);
        self.a_to_b_space_futex = AtomicU32::new(0);
        self.b_to_a_data_futex = AtomicU32::new(0);
        self.b_to_a_space_futex = AtomicU32::new(0);
        self._pad = [0; 48];
    }

    /// Validate the header and return the embedded configuration.
    pub fn validate(&self) -> Result<(), LayoutError> {
        if self.magic != MAGIC {
            return Err(LayoutError::InvalidMagic);
        }
        let major = self.version >> 16;
        let our_major = PROTOCOL_VERSION >> 16;
        if major != our_major {
            return Err(LayoutError::IncompatibleVersion {
                expected: PROTOCOL_VERSION,
                found: self.version,
            });
        }
        // Validate config fields
        if !self.ring_capacity.is_power_of_two() || self.ring_capacity == 0 {
            return Err(LayoutError::InvalidConfig(
                "ring_capacity must be non-zero power of 2",
            ));
        }
        if self.slot_size == 0 {
            return Err(LayoutError::InvalidConfig("slot_size must be > 0"));
        }
        if self.slot_count == 0 {
            return Err(LayoutError::InvalidConfig("slot_count must be > 0"));
        }
        Ok(())
    }

    /// Extract the configuration from a validated header.
    pub fn config(&self) -> (u32, u32, u32) {
        (self.ring_capacity, self.slot_size, self.slot_count)
    }
}

// =============================================================================
// Descriptor Ring
// =============================================================================

// DescRingHeader is now a type alias for shm_primitives::SpscRingHeader
// (see import at top of file). The layout is identical (192 bytes).
const _: () = assert!(core::mem::size_of::<DescRingHeader>() == 192);

/// A view into a descriptor ring in SHM.
///
/// This provides safe access to the ring operations. The actual descriptors
/// are stored immediately after the header in SHM.
///
/// Internally delegates to `shm_primitives::SpscRingRaw<MsgDescHot>` for
/// the lock-free algorithm, keeping the rapace-specific API intact.
pub struct DescRing {
    inner: SpscRingRaw<MsgDescHot>,
}

// SAFETY: DescRing is Send + Sync because it wraps SpscRingRaw which is
// Send + Sync for Send types, and MsgDescHot is Send.
unsafe impl Send for DescRing {}
unsafe impl Sync for DescRing {}

impl DescRing {
    /// Create a ring view from raw pointers.
    ///
    /// # Safety
    ///
    /// - `header` must point to a valid, initialized `DescRingHeader` in SHM.
    /// - `descriptors` must point to `header.capacity` initialized `MsgDescHot` slots.
    /// - The memory must remain valid for the lifetime of this `DescRing`.
    pub unsafe fn from_raw(header: *mut DescRingHeader, descriptors: *mut MsgDescHot) -> Self {
        Self {
            inner: unsafe { SpscRingRaw::from_raw(header, descriptors) },
        }
    }

    /// Enqueue a descriptor (producer side).
    ///
    /// `local_head` is producer-private (stack-local, not in SHM).
    /// On success, `local_head` is incremented.
    pub fn enqueue(&self, local_head: &mut u64, desc: &MsgDescHot) -> Result<(), RingError> {
        self.inner
            .enqueue(local_head, desc)
            .map_err(|_| RingError::Full)
    }

    /// Dequeue a descriptor (consumer side).
    pub fn dequeue(&self) -> Option<MsgDescHot> {
        self.inner.dequeue()
    }

    /// Check if the ring is empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.inner.is_empty()
    }

    /// Get the capacity of the ring.
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.inner.capacity()
    }

    /// Get the ring status (for diagnostics).
    ///
    /// Returns a snapshot of the ring's head/tail pointers and derived metrics.
    pub fn ring_status(&self) -> RingStatus {
        let status = self.inner.status();
        RingStatus {
            visible_head: status.visible_head,
            tail: status.tail,
            capacity: status.capacity,
            len: status.len,
        }
    }
}

/// Status snapshot of a descriptor ring.
#[derive(Debug, Clone, Copy)]
pub struct RingStatus {
    /// Producer's published head (items 0..visible_head have been enqueued).
    pub visible_head: u64,
    /// Consumer's tail (items 0..tail have been dequeued).
    pub tail: u64,
    /// Ring capacity.
    pub capacity: u32,
    /// Current length (visible_head - tail).
    pub len: u32,
}

impl std::fmt::Display for RingStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "head={} tail={} len={}/{} ({}%)",
            self.visible_head,
            self.tail,
            self.len,
            self.capacity,
            if self.capacity > 0 {
                self.len * 100 / self.capacity
            } else {
                0
            }
        )
    }
}

// =============================================================================
// Data Segment (Slab Allocator)
// =============================================================================

// SlotState, SlotMeta, and DataSegmentHeader are now re-exported from
// shm-primitives (see imports at top of file). The layouts are identical.
const _: () = assert!(core::mem::size_of::<SlotMeta>() == 8);
const _: () = assert!(core::mem::size_of::<DataSegmentHeader>() == 64);

/// A view into the data segment in SHM.
///
/// Internally delegates to `shm_primitives::TreiberSlabRaw` for the lock-free
/// slab allocator algorithm, keeping the rapace-specific API intact (including
/// futex signaling for backpressure).
pub struct DataSegment {
    inner: TreiberSlabRaw,
    header: *mut DataSegmentHeader,
    slot_meta: *mut SlotMeta,
    slot_data: *mut u8,
}

// SAFETY: DataSegment is Send + Sync because it wraps TreiberSlabRaw which is
// Send + Sync.
unsafe impl Send for DataSegment {}
unsafe impl Sync for DataSegment {}

impl DataSegment {
    /// Create a data segment view from raw pointers.
    ///
    /// # Safety
    ///
    /// - All pointers must be valid and properly aligned.
    /// - The memory must remain valid for the lifetime of this `DataSegment`.
    pub unsafe fn from_raw(
        header: *mut DataSegmentHeader,
        slot_meta: *mut SlotMeta,
        slot_data: *mut u8,
    ) -> Self {
        Self {
            inner: unsafe { TreiberSlabRaw::from_raw(header, slot_meta, slot_data) },
            header,
            slot_meta,
            slot_data,
        }
    }

    /// Get the header.
    #[inline]
    fn header(&self) -> &DataSegmentHeader {
        unsafe { &*self.header }
    }

    /// Get slot metadata.
    ///
    /// # Safety
    ///
    /// Index must be < slot_count.
    #[inline]
    unsafe fn meta(&self, index: u32) -> &SlotMeta {
        unsafe { &*self.slot_meta.add(index as usize) }
    }

    /// Get slot data pointer.
    ///
    /// # Safety
    ///
    /// Index must be < slot_count.
    #[inline]
    unsafe fn data_ptr(&self, index: u32) -> *mut u8 {
        let slot_size = self.header().slot_size as usize;
        unsafe { self.slot_data.add(index as usize * slot_size) }
    }

    /// Get slot data pointer (public version for allocator).
    ///
    /// # Safety
    ///
    /// Index must be < slot_count and the caller must own the slot.
    #[inline]
    pub unsafe fn data_ptr_public(&self, index: u32) -> *mut u8 {
        unsafe { self.data_ptr(index) }
    }

    /// Read the next_free link stored in the first 4 bytes of a slot's data.
    ///
    /// # Safety
    ///
    /// Index must be < slot_count and the slot must be in a free state.
    #[inline]
    unsafe fn get_slot_next_free(&self, index: u32) -> u32 {
        let ptr = unsafe { self.data_ptr(index) as *const u32 };
        unsafe { std::ptr::read_volatile(ptr) }
    }

    /// Initialize the free list by linking all slots together.
    ///
    /// This should be called once when creating a new SHM segment.
    ///
    /// # Safety
    ///
    /// Must only be called during segment initialization, before any
    /// concurrent access.
    pub unsafe fn init_free_list(&self) {
        unsafe { self.inner.init_free_list() }
    }

    /// Allocate a slot using lock-free pop from free list.
    ///
    /// Returns (slot_index, generation) on success.
    ///
    /// This is O(1) on the happy path (no contention).
    pub fn alloc(&self) -> Result<(u32, u32), SlotError> {
        match self.inner.try_alloc() {
            AllocResult::Ok(handle) => Ok((handle.index, handle.generation)),
            AllocResult::WouldBlock => Err(SlotError::NoFreeSlots),
        }
    }

    /// Mark a slot as in-flight (after enqueuing descriptor).
    pub fn mark_in_flight(&self, index: u32, expected_gen: u32) -> Result<(), SlotError> {
        let handle = SlotHandle {
            index,
            generation: expected_gen,
        };
        self.inner
            .mark_in_flight(handle)
            .map_err(convert_slot_error)
    }

    /// Free a slot (receiver side, after processing).
    ///
    /// After transitioning to Free state, the slot is pushed back onto the free list.
    pub fn free(&self, index: u32, expected_gen: u32) -> Result<(), SlotError> {
        let handle = SlotHandle {
            index,
            generation: expected_gen,
        };
        let result = self.inner.free(handle).map_err(convert_slot_error);
        if result.is_ok() {
            // Signal anyone waiting for slots
            super::futex::futex_signal(self.slot_available_futex());
        }
        result
    }

    /// Get the slot availability futex for backpressure signaling.
    #[inline]
    pub fn slot_available_futex(&self) -> &AtomicU32 {
        unsafe { &(*self.header).slot_available }
    }

    /// Free a slot that's still in Allocated state (never sent).
    ///
    /// This is used by the allocator when data is dropped before being sent.
    /// After transitioning to Free state, the slot is pushed back onto the free list.
    pub fn free_allocated(&self, index: u32, expected_gen: u32) -> Result<(), SlotError> {
        let handle = SlotHandle {
            index,
            generation: expected_gen,
        };
        let result = self
            .inner
            .free_allocated(handle)
            .map_err(convert_slot_error);
        if result.is_ok() {
            // Signal anyone waiting for slots
            super::futex::futex_signal(self.slot_available_futex());
        }
        result
    }

    /// Copy data into a slot.
    ///
    /// # Safety
    ///
    /// Caller must own the slot (Allocated state with matching generation).
    pub unsafe fn copy_to_slot(&self, index: u32, data: &[u8]) -> Result<(), SlotError> {
        let header = self.header();

        if index >= header.slot_count {
            return Err(SlotError::InvalidIndex);
        }

        if data.len() > header.slot_size as usize {
            return Err(SlotError::PayloadTooLarge {
                len: data.len(),
                max: header.slot_size as usize,
            });
        }

        // SAFETY: index < slot_count, data.len() <= slot_size.
        let dst = unsafe { self.data_ptr(index) };
        unsafe {
            std::ptr::copy_nonoverlapping(data.as_ptr(), dst, data.len());
        }

        Ok(())
    }

    /// Read data from a slot.
    ///
    /// # Safety
    ///
    /// Caller must have read access (InFlight state with matching generation).
    pub unsafe fn read_slot(
        &self,
        index: u32,
        expected_gen: u32,
        offset: u32,
        len: u32,
    ) -> Result<&[u8], SlotError> {
        let header = self.header();

        if index >= header.slot_count {
            return Err(SlotError::InvalidIndex);
        }

        // SAFETY: index < slot_count (checked above).
        let meta = unsafe { self.meta(index) };

        if meta.generation() != expected_gen {
            return Err(SlotError::StaleGeneration);
        }

        if meta.state() != SlotState::InFlight {
            return Err(SlotError::InvalidState);
        }

        let end = offset.saturating_add(len);
        if end > header.slot_size {
            return Err(SlotError::PayloadTooLarge {
                len: end as usize,
                max: header.slot_size as usize,
            });
        }

        // SAFETY: bounds checked above.
        let ptr = unsafe { self.data_ptr(index).add(offset as usize) };
        Ok(unsafe { std::slice::from_raw_parts(ptr, len as usize) })
    }

    /// Get slot size.
    #[inline]
    pub fn slot_size(&self) -> u32 {
        self.header().slot_size
    }

    /// Get slot count.
    #[inline]
    pub fn slot_count(&self) -> u32 {
        self.header().slot_count
    }

    /// Get slot status for debugging.
    ///
    /// Returns a struct with counts of slots in each state.
    pub fn slot_status(&self) -> SlotStatus {
        let slot_count = self.header().slot_count;
        let mut free = 0u32;
        let mut allocated = 0u32;
        let mut in_flight = 0u32;
        let mut unknown = 0u32;

        for i in 0..slot_count {
            // SAFETY: i < slot_count
            let meta = unsafe { self.meta(i) };
            match meta.state() {
                SlotState::Free => free += 1,
                SlotState::Allocated => allocated += 1,
                SlotState::InFlight => in_flight += 1,
            }
        }

        // Count free list length to verify consistency
        let mut free_list_len = 0u32;
        let header = unsafe { &*self.header };
        let mut current = {
            let (index, _tag) = unpack_free_head(header.free_head.load(Ordering::Acquire));
            index
        };
        while current != FREE_LIST_END && free_list_len < slot_count + 1 {
            free_list_len += 1;
            // SAFETY: current should be < slot_count if free list is consistent
            if current < slot_count {
                current = unsafe { self.get_slot_next_free(current) };
            } else {
                unknown += 1;
                break;
            }
        }

        SlotStatus {
            total: slot_count,
            free,
            allocated,
            in_flight,
            unknown,
            free_list_len,
        }
    }
}

/// Slot status for debugging.
#[derive(Debug, Clone, Copy)]
pub struct SlotStatus {
    /// Total number of slots.
    pub total: u32,
    /// Slots in Free state.
    pub free: u32,
    /// Slots in Allocated state.
    pub allocated: u32,
    /// Slots in InFlight state.
    pub in_flight: u32,
    /// Slots in unknown state (should be 0).
    pub unknown: u32,
    /// Length of free list (should match `free`).
    pub free_list_len: u32,
}

impl std::fmt::Display for SlotStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "slots: {}/{} free, {} allocated, {} in_flight (free_list_len={})",
            self.free, self.total, self.allocated, self.in_flight, self.free_list_len
        )?;
        if self.unknown > 0 {
            write!(f, ", {} UNKNOWN", self.unknown)?;
        }
        if self.free != self.free_list_len {
            write!(
                f,
                " [MISMATCH: free={} != free_list={}]",
                self.free, self.free_list_len
            )?;
        }
        Ok(())
    }
}

// =============================================================================
// Errors
// =============================================================================

/// Errors from layout validation.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LayoutError {
    /// Invalid magic bytes.
    InvalidMagic,
    /// Incompatible protocol version.
    IncompatibleVersion { expected: u32, found: u32 },
    /// Segment too small.
    SegmentTooSmall { required: usize, found: usize },
    /// Invalid configuration in header.
    InvalidConfig(&'static str),
}

impl std::fmt::Display for LayoutError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::InvalidMagic => write!(f, "invalid magic bytes"),
            Self::IncompatibleVersion { expected, found } => {
                write!(
                    f,
                    "incompatible version: expected {}.{}, found {}.{}",
                    expected >> 16,
                    expected & 0xFFFF,
                    found >> 16,
                    found & 0xFFFF
                )
            }
            Self::SegmentTooSmall { required, found } => {
                write!(
                    f,
                    "segment too small: need {} bytes, got {}",
                    required, found
                )
            }
            Self::InvalidConfig(msg) => write!(f, "invalid config: {}", msg),
        }
    }
}

impl std::error::Error for LayoutError {}

/// Errors from ring operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RingError {
    /// Ring is full.
    Full,
}

impl std::fmt::Display for RingError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Full => write!(f, "ring is full"),
        }
    }
}

impl std::error::Error for RingError {}

/// Errors from slot operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    /// No free slots available.
    NoFreeSlots,
    /// Invalid slot index.
    InvalidIndex,
    /// Generation mismatch (stale reference).
    StaleGeneration,
    /// Slot in unexpected state.
    InvalidState,
    /// Payload too large for slot.
    PayloadTooLarge { len: usize, max: usize },
}

impl std::fmt::Display for SlotError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::NoFreeSlots => write!(f, "no free slots available"),
            Self::InvalidIndex => write!(f, "invalid slot index"),
            Self::StaleGeneration => write!(f, "stale generation"),
            Self::InvalidState => write!(f, "invalid slot state"),
            Self::PayloadTooLarge { len, max } => {
                write!(f, "payload too large for slot: {} bytes, max {}", len, max)
            }
        }
    }
}

impl std::error::Error for SlotError {}

/// Convert shm_primitives::SlotError to our local SlotError.
fn convert_slot_error(e: shm_primitives::SlotError) -> SlotError {
    match e {
        shm_primitives::SlotError::InvalidIndex => SlotError::InvalidIndex,
        shm_primitives::SlotError::GenerationMismatch { .. } => SlotError::StaleGeneration,
        shm_primitives::SlotError::InvalidState { .. } => SlotError::InvalidState,
    }
}

// =============================================================================
// Layout Calculations
// =============================================================================

/// Calculate the total size needed for a SHM segment (checked).
///
/// Returns an error string describing where the overflow occurred.
pub fn calculate_segment_size_checked(
    ring_capacity: u32,
    slot_size: u32,
    slot_count: u32,
) -> Result<usize, &'static str> {
    let header_size = core::mem::size_of::<SegmentHeader>();
    let ring_header_size = core::mem::size_of::<DescRingHeader>();
    let desc_size = core::mem::size_of::<MsgDescHot>();
    let data_header_size = core::mem::size_of::<DataSegmentHeader>();
    let slot_meta_size = core::mem::size_of::<SlotMeta>();

    let ring_descs_size = (ring_capacity as usize)
        .checked_mul(desc_size)
        .ok_or("SHM size overflow (ring descs)")?;
    let ring_size = ring_header_size
        .checked_add(ring_descs_size)
        .ok_or("SHM size overflow (ring)")?;

    let slot_meta_total = slot_meta_size
        .checked_mul(slot_count as usize)
        .ok_or("SHM size overflow (slot meta)")?;
    let slot_data_total = (slot_size as usize)
        .checked_mul(slot_count as usize)
        .ok_or("SHM size overflow (slot data)")?;

    let mut total = header_size;
    total = total
        .checked_add(ring_size)
        .and_then(|v| v.checked_add(ring_size))
        .and_then(|v| v.checked_add(data_header_size))
        .and_then(|v| v.checked_add(slot_meta_total))
        .and_then(|v| v.checked_add(slot_data_total))
        .ok_or("SHM size overflow (total)")?;

    Ok(total)
}

/// Calculate the total size needed for a SHM segment.
pub fn calculate_segment_size(ring_capacity: u32, slot_size: u32, slot_count: u32) -> usize {
    calculate_segment_size_checked(ring_capacity, slot_size, slot_count)
        .expect("SHM segment size overflow")
}

/// Offsets within the SHM segment.
#[derive(Debug, Clone, Copy)]
pub struct SegmentOffsets {
    pub header: usize,
    pub ring_a_to_b_header: usize,
    pub ring_a_to_b_descs: usize,
    pub ring_b_to_a_header: usize,
    pub ring_b_to_a_descs: usize,
    pub data_header: usize,
    pub slot_meta: usize,
    pub slot_data: usize,
}

impl SegmentOffsets {
    /// Calculate offsets for given parameters.
    pub fn calculate(ring_capacity: u32, slot_count: u32) -> Self {
        Self::calculate_checked(ring_capacity, slot_count).expect("SHM offset overflow")
    }

    /// Calculate offsets for given parameters (checked).
    ///
    /// Returns an error string describing where the overflow occurred.
    pub fn calculate_checked(ring_capacity: u32, slot_count: u32) -> Result<Self, &'static str> {
        let header_size = core::mem::size_of::<SegmentHeader>();
        let ring_header_size = core::mem::size_of::<DescRingHeader>();
        let desc_size = core::mem::size_of::<MsgDescHot>();
        let data_header_size = core::mem::size_of::<DataSegmentHeader>();
        let slot_meta_size = core::mem::size_of::<SlotMeta>();

        let ring_descs_size = (ring_capacity as usize)
            .checked_mul(desc_size)
            .ok_or("SHM offset overflow (ring descs)")?;
        let slot_meta_total = slot_meta_size
            .checked_mul(slot_count as usize)
            .ok_or("SHM offset overflow (slot meta)")?;

        let header = 0usize;
        let ring_a_to_b_header = header
            .checked_add(header_size)
            .ok_or("SHM offset overflow (ring A->B header)")?;
        let ring_a_to_b_descs = ring_a_to_b_header
            .checked_add(ring_header_size)
            .ok_or("SHM offset overflow (ring A->B descs)")?;
        let ring_b_to_a_header = ring_a_to_b_descs
            .checked_add(ring_descs_size)
            .ok_or("SHM offset overflow (ring B->A header)")?;
        let ring_b_to_a_descs = ring_b_to_a_header
            .checked_add(ring_header_size)
            .ok_or("SHM offset overflow (ring B->A descs)")?;
        let data_header = ring_b_to_a_descs
            .checked_add(ring_descs_size)
            .ok_or("SHM offset overflow (data header)")?;
        let slot_meta = data_header
            .checked_add(data_header_size)
            .ok_or("SHM offset overflow (slot meta)")?;
        let slot_data = slot_meta
            .checked_add(slot_meta_total)
            .ok_or("SHM offset overflow (slot data)")?;

        Ok(Self {
            header,
            ring_a_to_b_header,
            ring_a_to_b_descs,
            ring_b_to_a_header,
            ring_b_to_a_descs,
            data_header,
            slot_meta,
            slot_data,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_segment_header_size() {
        assert_eq!(core::mem::size_of::<SegmentHeader>(), 128);
    }

    #[test]
    fn test_desc_ring_header_size() {
        assert_eq!(core::mem::size_of::<DescRingHeader>(), 192);
    }

    #[test]
    fn test_slot_meta_size() {
        assert_eq!(core::mem::size_of::<SlotMeta>(), 8);
    }

    #[test]
    fn test_data_segment_header_size() {
        assert_eq!(core::mem::size_of::<DataSegmentHeader>(), 64);
    }

    #[test]
    fn test_calculate_segment_size() {
        let size =
            calculate_segment_size(DEFAULT_RING_CAPACITY, DEFAULT_SLOT_SIZE, DEFAULT_SLOT_COUNT);
        // Rough sanity check
        assert!(size > 0);
        // Header (128) + 2 rings (2 * (192 + 256*64)) + data header (64) + meta (64*8) + data (64*4096)
        // = 128 + 2*(192 + 16384) + 64 + 512 + 262144
        // = 128 + 33152 + 64 + 512 + 262144 = 296000
        assert_eq!(size, 296000);
    }

    #[test]
    fn test_segment_offsets() {
        let offsets = SegmentOffsets::calculate(DEFAULT_RING_CAPACITY, DEFAULT_SLOT_COUNT);

        assert_eq!(offsets.header, 0);
        // Header is 128 bytes (includes config for auto-discovery)
        assert_eq!(offsets.ring_a_to_b_header, 128);
        assert_eq!(offsets.ring_a_to_b_descs, 128 + 192);
        // ring_a_to_b_descs + 256*64 = 320 + 16384 = 16704
        assert_eq!(offsets.ring_b_to_a_header, 320 + 16384);
        // etc.
    }

    #[test]
    fn test_segment_header_validate() {
        let mut header = unsafe { std::mem::zeroed::<SegmentHeader>() };
        header.init(DEFAULT_RING_CAPACITY, DEFAULT_SLOT_SIZE, DEFAULT_SLOT_COUNT);
        assert!(header.validate().is_ok());

        header.magic[0] = b'X';
        assert!(matches!(header.validate(), Err(LayoutError::InvalidMagic)));
    }
}
