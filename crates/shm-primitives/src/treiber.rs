use core::mem::{align_of, size_of};

use crate::region::Region;
use crate::slot::{SlotMeta, SlotState};
use crate::sync::{AtomicU32, AtomicU64, Ordering, spin_loop};

/// Sentinel value indicating end of free list.
pub const FREE_LIST_END: u32 = u32::MAX;

/// Slab header (64 bytes, cache-line aligned).
#[repr(C, align(64))]
pub struct TreiberSlabHeader {
    pub slot_size: u32,
    pub slot_count: u32,
    pub max_frame_size: u32,
    _pad: u32,

    /// Free list head: index (low 32 bits) + tag (high 32 bits).
    pub free_head: AtomicU64,

    /// Slot-availability futex word (unused by this crate, but reserved for parity).
    pub slot_available: AtomicU32,

    _pad2: [u8; 36],
}

const _: () = assert!(core::mem::size_of::<TreiberSlabHeader>() == 64);

impl TreiberSlabHeader {
    pub fn init(&mut self, slot_size: u32, slot_count: u32) {
        self.slot_size = slot_size;
        self.slot_count = slot_count;
        self.max_frame_size = slot_size;
        self._pad = 0;
        self.free_head = AtomicU64::new(pack_free_head(FREE_LIST_END, 0));
        self.slot_available = AtomicU32::new(0);
        self._pad2 = [0; 36];
    }
}

/// Handle to an allocated slot.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SlotHandle {
    pub index: u32,
    pub generation: u32,
}

/// Result of an allocation attempt.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AllocResult {
    Ok(SlotHandle),
    WouldBlock,
}

/// Errors returned by slot transitions.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SlotError {
    InvalidIndex,
    GenerationMismatch {
        expected: u32,
        actual: u32,
    },
    InvalidState {
        expected: SlotState,
        actual: SlotState,
    },
}

pub type FreeError = SlotError;

/// A lock-free slab allocator backed by a region.
pub struct TreiberSlab {
    region: Region,
    header_offset: usize,
    meta_offset: usize,
    data_offset: usize,
}

unsafe impl Send for TreiberSlab {}
unsafe impl Sync for TreiberSlab {}

impl TreiberSlab {
    /// Initialize a new slab at `header_offset` in the region.
    ///
    /// # Safety
    ///
    /// The region must be writable and exclusively owned during initialization.
    pub unsafe fn init(
        region: Region,
        header_offset: usize,
        slot_count: u32,
        slot_size: u32,
    ) -> Self {
        assert!(slot_count > 0, "slot_count must be > 0");
        assert!(
            slot_size >= size_of::<u32>() as u32,
            "slot_size must be >= 4"
        );
        assert!(
            header_offset % 64 == 0,
            "header_offset must be 64-byte aligned"
        );

        let meta_offset = align_up(
            header_offset + size_of::<TreiberSlabHeader>(),
            align_of::<SlotMeta>(),
        );
        let data_offset = align_up(
            meta_offset + (slot_count as usize * size_of::<SlotMeta>()),
            align_of::<u32>(),
        );
        let required = data_offset + (slot_count as usize * slot_size as usize);
        assert!(required <= region.len(), "region too small for slab");

        let header = region.get_mut::<TreiberSlabHeader>(header_offset);
        header.init(slot_size, slot_count);

        // Initialize slot metadata.
        for i in 0..slot_count {
            let meta = region.get_mut::<SlotMeta>(meta_offset + i as usize * size_of::<SlotMeta>());
            meta.init();
        }

        let slab = Self {
            region,
            header_offset,
            meta_offset,
            data_offset,
        };

        // Initialize free list by linking all slots together.
        slab.init_free_list();

        slab
    }

    /// Attach to an existing slab.
    ///
    /// # Safety
    ///
    /// The region must contain a valid, initialized slab header at `header_offset`.
    pub unsafe fn attach(region: Region, header_offset: usize) -> Result<Self, &'static str> {
        assert!(
            header_offset % 64 == 0,
            "header_offset must be 64-byte aligned"
        );
        let header = region.get::<TreiberSlabHeader>(header_offset);
        if header.slot_count == 0 {
            return Err("slot_count must be > 0");
        }
        if header.slot_size < size_of::<u32>() as u32 {
            return Err("slot_size must be >= 4");
        }

        let meta_offset = align_up(
            header_offset + size_of::<TreiberSlabHeader>(),
            align_of::<SlotMeta>(),
        );
        let data_offset = align_up(
            meta_offset + (header.slot_count as usize * size_of::<SlotMeta>()),
            align_of::<u32>(),
        );
        let required = data_offset + (header.slot_count as usize * header.slot_size as usize);
        if required > region.len() {
            return Err("region too small for slab");
        }

        Ok(Self {
            region,
            header_offset,
            meta_offset,
            data_offset,
        })
    }

    #[inline]
    fn header(&self) -> &TreiberSlabHeader {
        unsafe { self.region.get::<TreiberSlabHeader>(self.header_offset) }
    }

    #[inline]
    unsafe fn meta(&self, index: u32) -> &SlotMeta {
        let off = self.meta_offset + index as usize * size_of::<SlotMeta>();
        self.region.get::<SlotMeta>(off)
    }

    #[inline]
    unsafe fn data_ptr(&self, index: u32) -> *mut u8 {
        let slot_size = self.header().slot_size as usize;
        let off = self.data_offset + index as usize * slot_size;
        self.region.offset(off)
    }

    #[inline]
    unsafe fn read_next_free(&self, index: u32) -> u32 {
        let ptr = self.data_ptr(index) as *const u32;
        unsafe { core::ptr::read_volatile(ptr) }
    }

    #[inline]
    unsafe fn write_next_free(&self, index: u32, next: u32) {
        let ptr = self.data_ptr(index) as *mut u32;
        unsafe { core::ptr::write_volatile(ptr, next) };
    }

    unsafe fn init_free_list(&self) {
        let slot_count = self.header().slot_count;
        if slot_count == 0 {
            return;
        }

        for i in 0..slot_count - 1 {
            unsafe { self.write_next_free(i, i + 1) };
        }
        unsafe { self.write_next_free(slot_count - 1, FREE_LIST_END) };

        let header = unsafe { self.region.get_mut::<TreiberSlabHeader>(self.header_offset) };
        header
            .free_head
            .store(pack_free_head(0, 0), Ordering::Release);
    }

    /// Try to allocate a slot.
    pub fn try_alloc(&self) -> AllocResult {
        let header = self.header();

        loop {
            let old_head = header.free_head.load(Ordering::Acquire);
            let (index, tag) = unpack_free_head(old_head);

            if index == FREE_LIST_END {
                return AllocResult::WouldBlock;
            }

            let next = unsafe { self.read_next_free(index) };
            let new_head = pack_free_head(next, tag.wrapping_add(1));

            match header.free_head.compare_exchange_weak(
                old_head,
                new_head,
                Ordering::AcqRel,
                Ordering::Acquire,
            ) {
                Ok(_) => {
                    let meta = unsafe { self.meta(index) };
                    let result = meta.state.compare_exchange(
                        SlotState::Free as u32,
                        SlotState::Allocated as u32,
                        Ordering::AcqRel,
                        Ordering::Acquire,
                    );

                    if result.is_err() {
                        self.push_to_free_list(index);
                        spin_loop();
                        continue;
                    }

                    let generation = meta.generation.fetch_add(1, Ordering::AcqRel) + 1;
                    return AllocResult::Ok(SlotHandle { index, generation });
                }
                Err(_) => {
                    spin_loop();
                    continue;
                }
            }
        }
    }

    /// Mark a slot as in-flight (after enqueue).
    pub fn mark_in_flight(&self, handle: SlotHandle) -> Result<(), SlotError> {
        if handle.index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        let meta = unsafe { self.meta(handle.index) };
        let actual = meta.generation.load(Ordering::Acquire);
        if actual != handle.generation {
            return Err(SlotError::GenerationMismatch {
                expected: handle.generation,
                actual,
            });
        }

        let result = meta.state.compare_exchange(
            SlotState::Allocated as u32,
            SlotState::InFlight as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        result
            .map(|_| ())
            .map_err(|actual| SlotError::InvalidState {
                expected: SlotState::Allocated,
                actual: SlotState::from_u32(actual).unwrap_or(SlotState::Free),
            })
    }

    /// Free an in-flight slot and push it to the free list.
    pub fn free(&self, handle: SlotHandle) -> Result<(), SlotError> {
        if handle.index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        let meta = unsafe { self.meta(handle.index) };
        let actual = meta.generation.load(Ordering::Acquire);
        if actual != handle.generation {
            return Err(SlotError::GenerationMismatch {
                expected: handle.generation,
                actual,
            });
        }

        let result = meta.state.compare_exchange(
            SlotState::InFlight as u32,
            SlotState::Free as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        if result.is_ok() {
            self.push_to_free_list(handle.index);
            Ok(())
        } else {
            Err(SlotError::InvalidState {
                expected: SlotState::InFlight,
                actual: SlotState::from_u32(result.err().unwrap()).unwrap_or(SlotState::Free),
            })
        }
    }

    /// Free a slot that is still Allocated (never sent).
    pub fn free_allocated(&self, handle: SlotHandle) -> Result<(), SlotError> {
        if handle.index >= self.header().slot_count {
            return Err(SlotError::InvalidIndex);
        }

        let meta = unsafe { self.meta(handle.index) };
        let actual = meta.generation.load(Ordering::Acquire);
        if actual != handle.generation {
            return Err(SlotError::GenerationMismatch {
                expected: handle.generation,
                actual,
            });
        }

        let result = meta.state.compare_exchange(
            SlotState::Allocated as u32,
            SlotState::Free as u32,
            Ordering::AcqRel,
            Ordering::Acquire,
        );

        if result.is_ok() {
            self.push_to_free_list(handle.index);
            Ok(())
        } else {
            Err(SlotError::InvalidState {
                expected: SlotState::Allocated,
                actual: SlotState::from_u32(result.err().unwrap()).unwrap_or(SlotState::Free),
            })
        }
    }

    /// Return a pointer to the slot data.
    ///
    /// # Safety
    ///
    /// The handle must be valid and the slot must be allocated.
    pub unsafe fn slot_data_ptr(&self, handle: SlotHandle) -> *mut u8 {
        unsafe { self.data_ptr(handle.index) }
    }

    /// Returns the slot size in bytes.
    #[inline]
    pub fn slot_size(&self) -> u32 {
        self.header().slot_size
    }

    /// Returns the total number of slots.
    #[inline]
    pub fn slot_count(&self) -> u32 {
        self.header().slot_count
    }

    /// Approximate number of free slots.
    pub fn free_count_approx(&self) -> u32 {
        let slot_count = self.header().slot_count;
        let mut free_list_len = 0u32;
        let mut current = {
            let (index, _tag) = unpack_free_head(self.header().free_head.load(Ordering::Acquire));
            index
        };

        while current != FREE_LIST_END && free_list_len < slot_count + 1 {
            free_list_len += 1;
            if current < slot_count {
                current = unsafe { self.read_next_free(current) };
            } else {
                break;
            }
        }

        free_list_len
    }

    fn push_to_free_list(&self, index: u32) {
        let header = self.header();

        loop {
            let old_head = header.free_head.load(Ordering::Acquire);
            let (old_index, tag) = unpack_free_head(old_head);

            unsafe { self.write_next_free(index, old_index) };

            let new_head = pack_free_head(index, tag.wrapping_add(1));

            if header
                .free_head
                .compare_exchange_weak(old_head, new_head, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return;
            }
        }
    }
}

#[inline]
fn pack_free_head(index: u32, tag: u32) -> u64 {
    ((tag as u64) << 32) | (index as u64)
}

#[inline]
fn unpack_free_head(packed: u64) -> (u32, u32) {
    let index = packed as u32;
    let tag = (packed >> 32) as u32;
    (index, tag)
}

#[inline]
const fn align_up(value: usize, align: usize) -> usize {
    (value + (align - 1)) & !(align - 1)
}
