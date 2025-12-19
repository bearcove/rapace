#![cfg(all(test, feature = "loom"))]

use crate::region::HeapRegion;
use crate::spsc::SpscRing;
use crate::sync::{AtomicU32, Ordering, thread};
use crate::treiber::{AllocResult, SlotHandle, TreiberSlab};
use crate::{SlotMeta, SlotState};
use alloc::vec;
use loom::sync::Arc;

#[test]
fn spsc_ring_concurrent() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let ring: SpscRing<u64> = unsafe { SpscRing::init(region, 0, 4) };
        let ring = Arc::new(ring);

        let producer_ring = ring.clone();
        let producer_owner = region_owner.clone();
        let producer_thread = thread::spawn(move || {
            let _keep = producer_owner;
            let (mut producer, _) = producer_ring.split();
            for i in 0..3u64 {
                while producer.try_push(i).is_would_block() {
                    thread::yield_now();
                }
            }
        });

        let consumer_ring = ring.clone();
        let consumer_owner = region_owner.clone();
        let consumer_thread = thread::spawn(move || {
            let _keep = consumer_owner;
            let (_, mut consumer) = consumer_ring.split();
            let mut received = alloc::vec::Vec::new();
            while received.len() < 3 {
                if let Some(v) = consumer.try_pop() {
                    received.push(v);
                } else {
                    thread::yield_now();
                }
            }
            received
        });

        producer_thread.join().unwrap();
        let received = consumer_thread.join().unwrap();
        assert_eq!(received, vec![0, 1, 2]);
    });
}

#[test]
fn treiber_concurrent_alloc_free() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let slab = unsafe { TreiberSlab::init(region, 0, 4, 64) };
        let slab = Arc::new(slab);

        let t1_slab = slab.clone();
        let t1_owner = region_owner.clone();
        let t1 = thread::spawn(move || {
            let _keep = t1_owner;
            if let AllocResult::Ok(handle) = t1_slab.try_alloc() {
                t1_slab.free_allocated(handle).unwrap();
            }
        });

        let t2_slab = slab.clone();
        let t2_owner = region_owner.clone();
        let t2 = thread::spawn(move || {
            let _keep = t2_owner;
            if let AllocResult::Ok(handle) = t2_slab.try_alloc() {
                t2_slab.free_allocated(handle).unwrap();
            }
        });

        t1.join().unwrap();
        t2.join().unwrap();
    });
}

#[test]
fn treiber_no_double_alloc() {
    loom::model(|| {
        let region_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let region = region_owner.region();
        let slab = unsafe { TreiberSlab::init(region, 0, 2, 64) };
        let slab = Arc::new(slab);
        let counter = Arc::new(AtomicU32::new(0));

        let run = |slab: Arc<TreiberSlab>, counter: Arc<AtomicU32>, owner: Arc<HeapRegion>| {
            let _keep = owner;
            for _ in 0..2 {
                if let AllocResult::Ok(_handle) = slab.try_alloc() {
                    counter.fetch_add(1, Ordering::SeqCst);
                }
            }
        };

        let t1 = thread::spawn({
            let slab = slab.clone();
            let counter = counter.clone();
            let owner = region_owner.clone();
            move || run(slab, counter, owner)
        });

        let t2 = thread::spawn({
            let slab = slab.clone();
            let counter = counter.clone();
            let owner = region_owner.clone();
            move || run(slab, counter, owner)
        });

        t1.join().unwrap();
        t2.join().unwrap();

        assert!(counter.load(Ordering::SeqCst) <= 2);
    });
}

#[test]
fn slot_state_transitions() {
    loom::model(|| {
        let meta = Arc::new(SlotMeta {
            generation: AtomicU32::new(0),
            state: AtomicU32::new(SlotState::Free as u32),
        });

        let t1 = thread::spawn({
            let meta = meta.clone();
            move || meta.try_transition(SlotState::Free, SlotState::Allocated)
        });

        let t2 = thread::spawn({
            let meta = meta.clone();
            move || meta.try_transition(SlotState::Free, SlotState::Allocated)
        });

        let r1 = t1.join().unwrap();
        let r2 = t2.join().unwrap();
        assert!(r1.is_ok() != r2.is_ok());
    });
}

#[test]
fn alloc_enqueue_dequeue_free_cycle() {
    loom::model(|| {
        let slab_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let slab_region = slab_owner.region();
        let slab = unsafe { TreiberSlab::init(slab_region, 0, 4, 64) };
        let slab = Arc::new(slab);

        let ring_owner = Arc::new(HeapRegion::new_zeroed(4096));
        let ring_region = ring_owner.region();
        let ring: SpscRing<SlotHandle> = unsafe { SpscRing::init(ring_region, 0, 4) };
        let ring = Arc::new(ring);

        let producer_slab = slab.clone();
        let producer_ring = ring.clone();
        let producer_owner = (slab_owner.clone(), ring_owner.clone());
        let producer = thread::spawn(move || {
            let _keep = producer_owner;
            let (mut producer, _) = producer_ring.split();
            let handle = match producer_slab.try_alloc() {
                AllocResult::Ok(handle) => handle,
                AllocResult::WouldBlock => return,
            };
            unsafe {
                let ptr = producer_slab.slot_data_ptr(handle);
                core::ptr::write_bytes(ptr, 0xAB, 16);
            }
            producer_slab.mark_in_flight(handle).unwrap();
            while producer.try_push(handle).is_would_block() {
                thread::yield_now();
            }
        });

        let consumer_slab = slab.clone();
        let consumer_ring = ring.clone();
        let consumer_owner = (slab_owner.clone(), ring_owner.clone());
        let consumer = thread::spawn(move || {
            let _keep = consumer_owner;
            let (_, mut consumer) = consumer_ring.split();
            loop {
                if let Some(handle) = consumer.try_pop() {
                    consumer_slab.free(handle).unwrap();
                    break;
                }
                thread::yield_now();
            }
        });

        producer.join().unwrap();
        consumer.join().unwrap();
    });
}
