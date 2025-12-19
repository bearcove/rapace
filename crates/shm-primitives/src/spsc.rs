use core::mem::{align_of, size_of};
use core::ptr;

use crate::region::Region;
use crate::sync::{AtomicU64, Ordering};

/// SPSC ring header (192 bytes, cache-line aligned fields).
#[repr(C)]
pub struct SpscRingHeader {
    /// Producer publication index (written by producer, read by consumer).
    pub visible_head: AtomicU64,
    _pad1: [u8; 56],

    /// Consumer index (written by consumer, read by producer).
    pub tail: AtomicU64,
    _pad2: [u8; 56],

    /// Ring capacity (power of 2, immutable after init).
    pub capacity: u32,
    _pad3: [u8; 60],
}

#[cfg(not(feature = "loom"))]
const _: () = assert!(core::mem::size_of::<SpscRingHeader>() == 192);

impl SpscRingHeader {
    /// Initialize a new ring header.
    pub fn init(&mut self, capacity: u32) {
        assert!(capacity.is_power_of_two(), "capacity must be power of 2");
        self.visible_head = AtomicU64::new(0);
        self._pad1 = [0; 56];
        self.tail = AtomicU64::new(0);
        self._pad2 = [0; 56];
        self.capacity = capacity;
        self._pad3 = [0; 60];
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.visible_head.load(Ordering::Acquire);
        tail >= head
    }

    #[inline]
    pub fn mask(&self) -> u64 {
        self.capacity as u64 - 1
    }

    #[inline]
    pub fn is_full(&self, local_head: u64) -> bool {
        let tail = self.tail.load(Ordering::Acquire);
        local_head.wrapping_sub(tail) >= self.capacity as u64
    }

    #[inline]
    pub fn len(&self) -> u64 {
        let tail = self.tail.load(Ordering::Relaxed);
        let head = self.visible_head.load(Ordering::Acquire);
        head.saturating_sub(tail)
    }
}

/// A wait-free SPSC ring buffer in a shared memory region.
pub struct SpscRing<T> {
    region: Region,
    header_offset: usize,
    entries_offset: usize,
    _marker: core::marker::PhantomData<T>,
}

unsafe impl<T: Send> Send for SpscRing<T> {}
unsafe impl<T: Send> Sync for SpscRing<T> {}

impl<T: Copy> SpscRing<T> {
    /// Initialize a new ring in the region.
    ///
    /// # Safety
    ///
    /// The region must be writable and exclusively owned during initialization.
    pub unsafe fn init(region: Region, header_offset: usize, capacity: u32) -> Self {
        assert!(
            capacity.is_power_of_two() && capacity > 0,
            "capacity must be power of 2"
        );
        assert!(
            header_offset.is_multiple_of(64),
            "header_offset must be 64-byte aligned"
        );
        assert!(align_of::<T>() <= 64, "entry alignment must be <= 64");

        let entries_offset = header_offset + size_of::<SpscRingHeader>();
        let required = entries_offset + (capacity as usize * size_of::<T>());
        assert!(required <= region.len(), "region too small for ring");
        assert!(
            entries_offset.is_multiple_of(align_of::<T>()),
            "entries misaligned"
        );

        let header = unsafe { region.get_mut::<SpscRingHeader>(header_offset) };
        header.init(capacity);

        Self {
            region,
            header_offset,
            entries_offset,
            _marker: core::marker::PhantomData,
        }
    }

    /// Attach to an existing ring in the region.
    ///
    /// # Safety
    ///
    /// The region must contain a valid, initialized ring header.
    pub unsafe fn attach(region: Region, header_offset: usize) -> Self {
        assert!(
            header_offset.is_multiple_of(64),
            "header_offset must be 64-byte aligned"
        );
        assert!(align_of::<T>() <= 64, "entry alignment must be <= 64");

        let entries_offset = header_offset + size_of::<SpscRingHeader>();
        let header = unsafe { region.get::<SpscRingHeader>(header_offset) };
        let capacity = header.capacity;

        assert!(
            capacity.is_power_of_two() && capacity > 0,
            "invalid ring capacity"
        );
        let required = entries_offset + (capacity as usize * size_of::<T>());
        assert!(required <= region.len(), "region too small for ring");
        assert!(
            entries_offset.is_multiple_of(align_of::<T>()),
            "entries misaligned"
        );

        Self {
            region,
            header_offset,
            entries_offset,
            _marker: core::marker::PhantomData,
        }
    }

    #[inline]
    fn header(&self) -> &SpscRingHeader {
        unsafe { self.region.get::<SpscRingHeader>(self.header_offset) }
    }

    #[inline]
    unsafe fn entry_ptr(&self, slot: usize) -> *mut T {
        let base = self.region.offset(self.entries_offset);
        unsafe { base.add(slot * size_of::<T>()) as *mut T }
    }

    /// Split into producer and consumer handles.
    pub fn split(&self) -> (SpscProducer<'_, T>, SpscConsumer<'_, T>) {
        let head = self.header().visible_head.load(Ordering::Acquire);
        (
            SpscProducer {
                ring: self,
                local_head: head,
            },
            SpscConsumer { ring: self },
        )
    }

    /// Returns the ring capacity.
    #[inline]
    pub fn capacity(&self) -> u32 {
        self.header().capacity
    }

    /// Returns true if the ring appears empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.header().is_empty()
    }

    /// Returns a status snapshot of head/tail.
    pub fn status(&self) -> RingStatus {
        let header = self.header();
        let visible_head = header.visible_head.load(Ordering::Acquire);
        let tail = header.tail.load(Ordering::Acquire);
        let capacity = header.capacity;
        let len = visible_head.saturating_sub(tail) as u32;

        RingStatus {
            visible_head,
            tail,
            capacity,
            len,
        }
    }
}

/// Producer handle for the ring.
pub struct SpscProducer<'a, T> {
    pub(crate) ring: &'a SpscRing<T>,
    pub(crate) local_head: u64,
}

/// Consumer handle for the ring.
pub struct SpscConsumer<'a, T> {
    pub(crate) ring: &'a SpscRing<T>,
}

/// Result of a push attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushResult {
    Ok,
    WouldBlock,
}

impl PushResult {
    #[inline]
    pub fn is_would_block(self) -> bool {
        matches!(self, PushResult::WouldBlock)
    }
}

impl<'a, T: Copy> SpscProducer<'a, T> {
    /// Try to push an entry to the ring.
    pub fn try_push(&mut self, entry: T) -> PushResult {
        let header = self.ring.header();
        let capacity = header.capacity as u64;
        let mask = header.mask();

        let tail = header.tail.load(Ordering::Acquire);
        if self.local_head.wrapping_sub(tail) >= capacity {
            return PushResult::WouldBlock;
        }

        let slot = (self.local_head & mask) as usize;
        unsafe {
            let ptr = self.ring.entry_ptr(slot);
            ptr::write(ptr, entry);
        }

        self.local_head = self.local_head.wrapping_add(1);
        header
            .visible_head
            .store(self.local_head, Ordering::Release);

        PushResult::Ok
    }

    /// Returns true if the ring appears full.
    #[inline]
    pub fn is_full(&self) -> bool {
        self.ring.header().is_full(self.local_head)
    }

    /// Returns the number of entries that can be pushed (approximate).
    #[inline]
    pub fn available_capacity(&self) -> u64 {
        let capacity = self.ring.header().capacity as u64;
        let tail = self.ring.header().tail.load(Ordering::Acquire);
        capacity.saturating_sub(self.local_head.wrapping_sub(tail))
    }
}

impl<'a, T: Copy> SpscConsumer<'a, T> {
    /// Try to pop an entry from the ring.
    pub fn try_pop(&mut self) -> Option<T> {
        let header = self.ring.header();
        let tail = header.tail.load(Ordering::Relaxed);
        let head = header.visible_head.load(Ordering::Acquire);

        if tail == head {
            return None;
        }

        let mask = header.mask();
        let slot = (tail & mask) as usize;
        let entry = unsafe { ptr::read(self.ring.entry_ptr(slot)) };
        header.tail.store(tail.wrapping_add(1), Ordering::Release);

        Some(entry)
    }

    /// Returns true if the ring appears empty.
    #[inline]
    pub fn is_empty(&self) -> bool {
        self.ring.is_empty()
    }

    /// Returns the number of entries available to pop (approximate).
    #[inline]
    pub fn len(&self) -> u64 {
        self.ring.header().len()
    }
}

/// Status snapshot of a ring.
#[derive(Debug, Clone, Copy)]
pub struct RingStatus {
    pub visible_head: u64,
    pub tail: u64,
    pub capacity: u32,
    pub len: u32,
}
