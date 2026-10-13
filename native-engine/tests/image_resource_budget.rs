use crate::revision::image_resource_budget::{
    shared_image_resource_budget, ImageResourceBudget, ImageResourceError, MAX_BYTES,
};
use std::sync::{Arc, Barrier};

#[test]
#[cfg(feature = "reference_image_asset")]
fn owned_bytes_have_exact_layout_and_release_storage_before_quota() {
    use crate::reference_image_asset::measure_allocations;
    let budget = ImageResourceBudget::isolated(19).unwrap();
    let layout = std::alloc::Layout::array::<u8>(19).unwrap();
    let (mut row, calls) = measure_allocations(Some(layout), || budget.allocate_bytes(19).unwrap());
    assert_eq!(calls, [1, 0, 0]);
    assert_eq!(row.as_mut(), [0; 19]);
    row.as_mut().copy_from_slice(&[7; 19]);
    assert_eq!(budget.used(), Ok(19));
    let (refused, calls) = measure_allocations(Some(layout), || budget.allocate_bytes(19));
    assert!(matches!(refused, Err(ImageResourceError::Capacity)));
    assert_eq!(calls, [0, 0, 0]);
    let (_, calls) = measure_allocations(Some(layout), || drop(row));
    assert_eq!(calls, [0, 1, 0]);
    assert_eq!(budget.used(), Ok(0));
    assert!(budget.allocate_bytes(0).is_err());
    assert!(budget.allocate_bytes(20).is_err());
}

#[test]
fn typed_layout_cost_null_and_one_shot_initialization_are_checked() {
    #[repr(align(64))]
    struct Aligned(u8);
    let cost = std::mem::size_of::<Aligned>();
    let budget = ImageResourceBudget::new(cost * 2).unwrap();
    let mut charge = budget.reserve(cost).unwrap();
    let called = std::cell::Cell::new(false);
    assert!(matches!(
        charge.allocate_value::<()>(|| {
            called.set(true);
            Ok(())
        }),
        Err(ImageResourceError::Capacity)
    ));
    assert!(!called.get());
    assert!(matches!(
        charge.allocate_value::<u8>(|| {
            called.set(true);
            Ok(1)
        }),
        Err(ImageResourceError::Capacity)
    ));
    assert!(!called.get());
    // SAFETY: null is explicitly permitted by the private allocation contract.
    unsafe fn null(_: std::alloc::Layout) -> *mut u8 {
        std::ptr::null_mut()
    }
    assert!(matches!(
        unsafe {
            charge.allocate_value_using(
                || {
                    called.set(true);
                    Ok(Aligned(1))
                },
                null,
            )
        },
        Err(ImageResourceError::Allocation)
    ));
    assert!(!called.get());
    assert!(matches!(
        charge.allocate_value(|| Ok(Aligned(1))),
        Err(ImageResourceError::Unavailable)
    ));
    drop(charge);
    let mut charge = budget.reserve(cost).unwrap();
    let value = charge.allocate_value(|| Ok(Aligned(7))).unwrap();
    assert_eq!(value.0, 7);
    assert_eq!((&*value as *const Aligned as usize) % 64, 0);
    assert!(matches!(
        charge.allocate_value(|| Ok(Aligned(9))),
        Err(ImageResourceError::Unavailable)
    ));
    drop(value);
    assert_eq!(budget.used(), Ok(cost));
    drop(charge);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
#[cfg(feature = "reference_image_asset")]
fn typed_initializer_and_destructor_failures_deallocate_before_charge_release() {
    use crate::reference_image_asset::measure_allocations;
    #[repr(align(64))]
    struct Value<'a> {
        budget: &'a ImageResourceBudget,
        panic_drop: bool,
    }
    impl Drop for Value<'_> {
        fn drop(&mut self) {
            assert_eq!(self.budget.used(), Ok(std::mem::size_of::<Self>()));
            if self.panic_drop {
                panic!("typed destructor unwind");
            }
        }
    }
    let layout = std::alloc::Layout::new::<Value<'_>>();
    let budget = ImageResourceBudget::new(layout.size()).unwrap();
    for stage in 0..4 {
        let mut charge = budget.reserve(layout.size()).unwrap();
        let (result, calls) = measure_allocations(Some(layout), || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                let value = charge.allocate_value(|| match stage {
                    0 => Err(ImageResourceError::Allocation),
                    1 => panic!("typed initializer unwind"),
                    _ => Ok(Value {
                        budget: &budget,
                        panic_drop: stage == 3,
                    }),
                });
                drop(value);
            }))
        });
        assert_eq!(calls, [1, 1, 0], "one exact Box allocated and destroyed");
        assert_eq!(result.is_err(), stage == 1 || stage == 3);
        assert_eq!(budget.used(), Ok(layout.size()));
        drop(charge);
        assert_eq!(budget.used(), Ok(0));
    }
}

#[test]
fn fixed_shared_domain_and_scalar_overlap_boundaries() {
    assert!(std::ptr::eq(
        shared_image_resource_budget(),
        shared_image_resource_budget()
    ));
    assert_eq!(shared_image_resource_budget().limit, 384 * 1024 * 1024);
    assert!(matches!(
        ImageResourceBudget::new(0),
        Err(ImageResourceError::Limit)
    ));
    assert!(matches!(
        ImageResourceBudget::new(MAX_BYTES + 1),
        Err(ImageResourceError::Limit)
    ));
    let budget = ImageResourceBudget::new(MAX_BYTES).unwrap();
    let cpu = budget.reserve(256 * 1024 * 1024).unwrap();
    assert!(matches!(
        budget.reserve(256 * 1024 * 1024),
        Err(ImageResourceError::Capacity)
    ));
    let other = budget.reserve(128 * 1024 * 1024).unwrap();
    assert_eq!(budget.used(), Ok(MAX_BYTES));
    assert!(matches!(
        budget.reserve(1),
        Err(ImageResourceError::Capacity)
    ));
    drop(cpu);
    assert_eq!(budget.used(), Ok(128 * 1024 * 1024));
    drop(other);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn exact_initialized_capacity_and_one_shot_allocation() {
    let budget = ImageResourceBudget::new(19).unwrap();
    let mut charge = budget.reserve(19).unwrap();
    let mut bytes = charge.allocate().unwrap();
    assert_eq!((bytes.len(), bytes.capacity()), (19, 19));
    assert_eq!(bytes, [0; 19]);
    bytes.copy_from_slice(&[7; 19]);
    assert_eq!(charge.allocate(), Err(ImageResourceError::Unavailable));
    assert_eq!(budget.used(), Ok(19));
    drop(bytes);
    assert_eq!(budget.used(), Ok(19));
    drop(charge);
    assert_eq!(budget.used(), Ok(0));
}

#[test]
fn failed_allocation_and_unwind_release_the_held_charge() {
    let budget = ImageResourceBudget::new(8).unwrap();
    for unwind in [false, true] {
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            let mut charge = budget.reserve(8).unwrap();
            charge.allocate_after(|| {
                assert_eq!(budget.used(), Ok(8));
                if unwind {
                    panic!("allocation hook unwind");
                }
                Err(ImageResourceError::Allocation)
            })
        }));
        if unwind {
            assert!(result.is_err());
        } else {
            assert_eq!(result.unwrap(), Err(ImageResourceError::Allocation));
        }
        assert_eq!(budget.used(), Ok(0));
    }
}

#[test]
fn checked_overflow_and_zero_refusal_do_not_mutate_accounting() {
    let budget = ImageResourceBudget::new(8).unwrap();
    assert!(budget.reserve(0).is_err());
    assert!(budget.reserve(usize::MAX).is_err());
    assert_eq!(budget.used(), Ok(0));
    *budget.used.lock().unwrap() = usize::MAX;
    assert!(matches!(
        budget.reserve(1),
        Err(ImageResourceError::Capacity)
    ));
    assert_eq!(budget.used(), Ok(usize::MAX));
}

#[test]
fn poisoned_domain_refuses_and_drop_does_not_reopen_it() {
    let budget = ImageResourceBudget::new(8).unwrap();
    let mut charge = budget.reserve(8).unwrap();
    let _ = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        let _guard = budget.used.lock().unwrap();
        panic!("poison quota");
    }));
    assert_eq!(charge.allocate(), Err(ImageResourceError::Unavailable));
    assert!(matches!(
        budget.reserve(1),
        Err(ImageResourceError::Unavailable)
    ));
    drop(charge);
    assert_eq!(budget.used(), Err(ImageResourceError::Unavailable));
    assert_eq!(*budget.used.lock().unwrap_err().into_inner(), 8);
}

#[test]
fn simultaneous_producers_cannot_overbook_a_shared_domain() {
    let budget = ImageResourceBudget::new(8).unwrap();
    let ready = Arc::new(Barrier::new(3));
    let release = Arc::new(Barrier::new(3));
    let handles: Vec<_> = (0..2)
        .map(|_| {
            let (budget, ready, release) = (budget.clone(), ready.clone(), release.clone());
            std::thread::spawn(move || {
                let charge = budget.reserve(6);
                ready.wait();
                release.wait();
                charge.is_ok()
            })
        })
        .collect();
    ready.wait();
    assert_eq!(budget.used(), Ok(6));
    release.wait();
    let admitted = handles
        .into_iter()
        .map(|h| usize::from(h.join().unwrap()))
        .sum::<usize>();
    assert_eq!(admitted, 1);
    assert_eq!(budget.used(), Ok(0));
}
