use super::*;
use object_codec::encoded_wire_test::*;
use object_codec::{ProjectReferenceState, ProjectWireError};
use std::sync::{Arc, Weak};

#[test]
fn actual_production_wire_allocates_one_workspace_and_preserves_encoded_identity() {
    use crate::reference_image_asset::measure_allocations;
    let bytes = serde_json::to_vec(&wire()).unwrap();
    let (result, calls) =
        measure_allocations(Some(std::alloc::Layout::new::<PngWorkspace>()), || {
            ObjectSnapshot::from_managed_project_wire("production".into(), &bytes, limits())
        });
    assert_eq!(calls, [1, 1, 0]);
    let original = result.unwrap();
    let cpu = ImageResourceBudget::isolated(std::mem::size_of::<PngWorkspace>()).unwrap();
    let encoded = ObjectSnapshot::encoded_test_budget(1024);
    let snapshot = ObjectSnapshot::managed_project_wire_using(
        "test".into(),
        &bytes,
        limits(),
        |n| encoded.reserve(n).map_err(|_| "reserve"),
        |p| p.allocate().map_err(|_| "allocate"),
        |p, source| {
            assert_eq!(cpu.used(), Ok(0), "workspace freed before bind");
            p.bind(source).map_err(|_| "bind")
        },
        ObjectSnapshot::from_admitted_project,
        |source, png| {
            object_codec::validate_png_with_workspace(source, png, || {
                charged_png_workspace(&cpu, || {
                    assert_eq!(cpu.used(), Ok(std::mem::size_of::<PngWorkspace>()));
                    Ok(PngWorkspace::new())
                })
                .map_err(|_| ProjectWireError::Budget)
            })
            .map(|_| ())
        },
    )
    .unwrap();
    assert_eq!(snapshot.project(), original.project());
    let ProjectReferenceState::StillImage(image) = snapshot.project().unwrap().reference_state()
    else {
        panic!()
    };
    assert_eq!(
        image.encoded_allocation().len(),
        image.encoded_allocation().capacity()
    );
    let weak = Arc::downgrade(image.encoded_allocation());
    drop(image);
    drop(snapshot);
    assert!(weak.upgrade().is_none());
    assert_eq!(cpu.used(), Ok(0));
    assert_eq!(encoded.used(), Ok(0));
}

#[test]
fn no_media_semantic_base64_and_header_failures_never_initialize_workspace() {
    let cpu = ImageResourceBudget::isolated(std::mem::size_of::<PngWorkspace>()).unwrap();
    let encoded = ObjectSnapshot::encoded_test_budget(1024);
    let mut invalid_settings = wire();
    invalid_settings["viewportReference"]["opacity"] = 2.into();
    let mut invalid_base64 = wire();
    invalid_base64["assets"][0]["data"] = "!!!!".into();
    let mut invalid_header = wire();
    invalid_header["assets"][0]["data"] = "AAAA".into();
    let mut no_media = wire();
    no_media["viewportReference"] = serde_json::Value::Null;
    no_media["assets"] = serde_json::json!([]);
    for data in [no_media, invalid_settings, invalid_base64, invalid_header] {
        let result = ObjectSnapshot::managed_project_wire_using(
            "early".into(),
            &serde_json::to_vec(&data).unwrap(),
            limits(),
            |n| encoded.reserve(n).map_err(|_| "reserve"),
            |p| p.allocate().map_err(|_| "allocate"),
            |_, _| panic!("early/no-media inputs never bind"),
            ObjectSnapshot::from_admitted_project,
            |source, png| {
                object_codec::validate_png_with_workspace(source, png, || {
                    charged_png_workspace(&cpu, || panic!("must refuse before initialization"))
                        .map_err(|_| ProjectWireError::Budget)
                })
                .map(|_| ())
            },
        );
        if data["viewportReference"].is_null() {
            assert!(result.is_ok());
        } else {
            assert!(result.is_err());
        }
        drop(result);
        assert_eq!(cpu.used(), Ok(0));
        assert_eq!(encoded.used(), Ok(0));
    }
}

fn crc(bytes: &[u8]) -> [u8; 4] {
    let mut value = !0u32;
    for &byte in bytes {
        value ^= u32::from(byte);
        for _ in 0..8 {
            value = if value & 1 == 1 {
                (value >> 1) ^ 0xedb88320
            } else {
                value >> 1
            };
        }
    }
    (!value).to_be_bytes()
}
#[test]
fn workspace_refusal_initializer_unwind_and_checksum_error_do_not_bind_or_leak() {
    use crate::reference_image_asset::measure_allocations;
    use base64::Engine;
    let layout = std::alloc::Layout::new::<PngWorkspace>();
    let cpu = ImageResourceBudget::isolated(layout.size()).unwrap();
    let encoded = ObjectSnapshot::encoded_test_budget(1024);
    let mut corrupt = base64::prelude::BASE64_STANDARD.decode(PNG).unwrap();
    corrupt[66] ^= 1;
    let checksum = crc(&corrupt[37..67]);
    corrupt[67..71].copy_from_slice(&checksum);
    for stage in 0..4 {
        let full = if stage == 0 {
            Some(cpu.reserve(layout.size()).unwrap())
        } else {
            None
        };
        let mut data = wire();
        if stage == 3 {
            data["assets"][0]["data"] = base64::prelude::BASE64_STANDARD.encode(&corrupt).into();
        }
        let bytes = serde_json::to_vec(&data).unwrap();
        let (result, calls) = measure_allocations(Some(layout), || {
            std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
                ObjectSnapshot::managed_project_wire_using(
                    "failure".into(),
                    &bytes,
                    limits(),
                    |n| encoded.reserve(n).map_err(|_| "reserve"),
                    |p| p.allocate().map_err(|_| "allocate"),
                    |_, _| panic!("workspace failure must not bind"),
                    |_, _| panic!("no identity on failure"),
                    |source, png| {
                        object_codec::validate_png_with_workspace(source, png, || {
                            charged_png_workspace(&cpu, || match stage {
                                1 => Err(ImageResourceError::Allocation),
                                2 => panic!("initializer unwind"),
                                _ => Ok(PngWorkspace::new()),
                            })
                            .map_err(|_| ProjectWireError::Budget)
                        })
                        .map(|_| ())
                    },
                )
            }))
        });
        assert_eq!(calls, if stage == 0 { [0, 0, 0] } else { [1, 1, 0] });
        if stage == 2 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        drop(full);
        assert_eq!(cpu.used(), Ok(0));
        assert_eq!(encoded.used(), Ok(0));
    }
}
#[test]
fn actual_managed_wire_entry_and_old_owner_pins_share_the_exact_blob() {
    let bytes = serde_json::to_vec(&wire()).unwrap();
    let snapshot =
        ObjectSnapshot::from_managed_project_wire("actual".into(), &bytes, limits()).unwrap();
    let ProjectReferenceState::StillImage(data) = snapshot.project().unwrap().reference_state()
    else {
        panic!()
    };
    let blob = data.encoded_allocation();
    assert_eq!(blob.len(), blob.capacity());
    assert_eq!(&blob[..8], b"\x89PNG\r\n\x1a\n");
    let weak = Arc::downgrade(blob);
    drop(data);
    let mut owner = crate::revision::ObjectRevisionOwner::from_admitted(snapshot.clone());
    owner.publish(snapshot.document().clone()).unwrap();
    owner.restore(0).unwrap();
    let pin = owner.acquire(0).unwrap();
    assert!(std::ptr::eq(
        pin.project().unwrap(),
        snapshot.project().unwrap()
    ));
    drop(owner);
    drop(snapshot);
    assert!(weak.upgrade().is_some());
    drop(pin);
    assert!(weak.upgrade().is_none());
}
#[test]
fn literal_wire_quota_decode_bind_identity_and_unwind_failures_are_atomic() {
    let budget = ObjectSnapshot::encoded_test_budget(1024);
    let calls = std::cell::Cell::new(0);
    let data = wire();
    let bytes = serde_json::to_vec(&data).unwrap();
    let entry = |bytes: &[u8]| {
        ObjectSnapshot::managed_project_wire(
            "test".into(),
            bytes,
            limits(),
            |n| budget.reserve(n).map_err(|_| "quota"),
            |p| counted(&calls, || p.allocate().map_err(|_| "alloc")),
            |p, a| p.bind(a).map_err(|_| "bind"),
            ObjectSnapshot::from_admitted_project,
        )
    };
    let mut malformed = data.clone();
    for changed in ["!!!!", "AAAA", &PNG[..PNG.len() - 4]] {
        malformed["assets"][0]["data"] = changed.into();
        assert!(entry(&serde_json::to_vec(&malformed).unwrap()).is_err());
        assert_eq!(budget.used(), Ok(0));
    }
    malformed = data.clone();
    malformed["viewportReference"]["opacity"] = 2.into();
    assert!(entry(&serde_json::to_vec(&malformed).unwrap()).is_err());
    assert_eq!(budget.used(), Ok(0));
    let full = budget.reserve(1024).unwrap();
    let before = calls.get();
    assert_eq!(entry(&bytes).unwrap_err(), ProjectWireError::Budget);
    assert_eq!(calls.get(), before);
    drop(full);
    let snapshot = entry(&bytes).unwrap();
    let cost = budget.used().unwrap();
    assert!(cost > 0);
    let remaining = budget.reserve(1024 - cost).unwrap();
    assert_eq!(entry(&bytes).unwrap_err(), ProjectWireError::Budget);
    drop(remaining);
    let escaped = snapshot.project().unwrap().clone();
    drop(snapshot);
    assert_eq!(budget.used(), Ok(cost));
    drop(escaped);
    assert_eq!(budget.used(), Ok(0));
    // The observer drops BEFORE its actual permit and sees no surviving blob.
    for stage in 0..4 {
        let weak = std::cell::RefCell::new(Weak::new());
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            ObjectSnapshot::managed_project_wire(
                "failure".into(),
                &bytes,
                limits(),
                |n| {
                    Ok(EncodedDropProbe {
                        permit: budget.reserve(n).map_err(|_| "quota")?,
                        weak: &weak,
                    })
                },
                |p| match stage {
                    0 => Err("alloc"),
                    _ => p.permit.allocate().map_err(|_| "alloc"),
                },
                |p, a| {
                    *weak.borrow_mut() = Arc::downgrade(a);
                    match stage {
                        1 => Err("bind"),
                        _ => p.permit.bind(a).map_err(|_| "bind"),
                    }
                },
                |_, _project| match stage {
                    3 => panic!("identity unwind"),
                    _ => Err("identity"),
                },
            )
        }));
        if stage == 3 {
            assert!(result.is_err());
        } else {
            assert!(result.unwrap().is_err());
        }
        assert_eq!(budget.used(), Ok(0));
    }
    let mut empty = data;
    empty["viewportReference"] = serde_json::Value::Null;
    empty["assets"] = serde_json::json!([]);
    let no_media = entry(&serde_json::to_vec(&empty).unwrap()).unwrap();
    assert!(no_media.project().is_some());
    assert_eq!(budget.used(), Ok(0));
    assert!(ObjectSnapshot::managed_project_wire(
        "".into(),
        &bytes,
        limits(),
        |_| panic!("empty instance before reservation"),
        |_: &mut ()| unreachable!(),
        |_, _| unreachable!(),
        |_, _| unreachable!()
    )
    .is_err());
}
