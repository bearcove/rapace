#![no_std]

#[cfg(any(test, feature = "alloc"))]
extern crate alloc;
#[cfg(feature = "std")]
extern crate std;

pub mod region;
pub mod slot;
pub mod spsc;
pub mod sync;
pub mod treiber;

#[cfg(any(test, feature = "alloc"))]
pub use region::HeapRegion;
pub use region::Region;
pub use slot::{SlotMeta, SlotState};
pub use spsc::{PushResult, SpscConsumer, SpscProducer, SpscRing, SpscRingHeader};
pub use treiber::{AllocResult, FreeError, SlotError, SlotHandle, TreiberSlab, TreiberSlabHeader};

#[cfg(all(test, feature = "loom"))]
mod loom_tests;
