//! A state the transaction publishes must still serialize through the one save
//! path. The preparation dry-runs the next project with the wire limits of
//! `serialize_project_file`, so three states that the encoded-retention ledger and the
//! 1024 catalog count alone would accept are refused before anything is bound or
//! published: an asset id equal to the viewport reference id, a catalog that with a
//! separate reference exceeds the 1024 wire entries, and a catalog whose base64
//! exceeds the 32 MiB envelope. Every refusal leaves revision, history and the saved
//! bytes exactly as they were.
use crate::history::NativeObjectHistory;
use crate::request_receipts::DispatchErrorCode;
use crate::revision::{ImageResourceBudget, ObjectSnapshot};
use serde_json::{json, Value};
use std::cell::Cell;

include!("object_raster_asset_attach_support.rs");

const UNSAVEABLE: (DispatchErrorCode, &str) =
    (DispatchErrorCode::Unavailable, "project_unsaveable");

fn crc32(bytes: &[u8]) -> u32 {
    let mut table = [0u32; 256];
    for n in 0..256u32 {
        let mut c = n;
        for _ in 0..8 {
            c = if c & 1 == 1 {
                0xEDB8_8320 ^ (c >> 1)
            } else {
                c >> 1
            };
        }
        table[n as usize] = c;
    }
    let mut crc = 0xFFFF_FFFFu32;
    for byte in bytes {
        crc = table[((crc ^ u32::from(*byte)) & 0xFF) as usize] ^ (crc >> 8);
    }
    crc ^ 0xFFFF_FFFF
}

fn adler32(bytes: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for block in bytes.chunks(5552) {
        for byte in block {
            a += u32::from(*byte);
            b += a;
        }
        a %= 65521;
        b %= 65521;
    }
    (b << 16) | a
}

fn chunk(out: &mut Vec<u8>, kind: &[u8; 4], data: &[u8]) {
    out.extend_from_slice(&(data.len() as u32).to_be_bytes());
    let mut body = kind.to_vec();
    body.extend_from_slice(data);
    out.extend_from_slice(&body);
    out.extend_from_slice(&crc32(&body).to_be_bytes());
}

/// A valid 8-bit RGBA PNG of one repeated byte, deflated with stored blocks only
/// (about `4 * width * height` bytes); built here with its own CRC-32 and Adler-32
/// so the size is exact and no encoder is involved.
fn solid_png(width: u32, height: u32, fill: u8) -> Vec<u8> {
    let mut raw = Vec::with_capacity((1 + width as usize * 4) * height as usize);
    for _ in 0..height {
        raw.push(0);
        raw.resize(raw.len() + width as usize * 4, fill);
    }
    let blocks: Vec<&[u8]> = raw.chunks(65535).collect();
    let mut zlib = vec![0x78, 0x01];
    for (index, block) in blocks.iter().enumerate() {
        zlib.push(u8::from(index + 1 == blocks.len()));
        zlib.extend_from_slice(&(block.len() as u16).to_le_bytes());
        zlib.extend_from_slice(&(!(block.len() as u16)).to_le_bytes());
        zlib.extend_from_slice(block);
    }
    zlib.extend_from_slice(&adler32(&raw).to_be_bytes());
    let mut header = Vec::new();
    header.extend_from_slice(&width.to_be_bytes());
    header.extend_from_slice(&height.to_be_bytes());
    header.extend_from_slice(&[8, 6, 0, 0, 0]);
    let mut png = vec![0x89, b'P', b'N', b'G', 0x0D, 0x0A, 0x1A, 0x0A];
    chunk(&mut png, b"IHDR", &header);
    chunk(&mut png, b"IDAT", &zlib);
    chunk(&mut png, b"IEND", &[]);
    png
}

#[test]
fn the_hand_built_png_is_accepted_by_the_real_validator() {
    let mut owner = open_history();
    let png = solid_png(3, 2, 7);
    let request = attach_request(&owner, "tiny", "solid.png", "image/png");
    let response = owner.dispatch_asset_command(&request, Some(&png)).unwrap();
    assert!(response.is_ok(), "hand-built png refused: {response:?}");
    assert_eq!(
        records_of(&owner)[0]
            .source()
            .encoded_allocation()
            .unwrap()
            .as_slice(),
        &png[..]
    );
}

#[test]
fn a_new_asset_whose_minted_id_is_the_viewport_reference_id_is_refused_and_the_project_still_saves()
{
    let mut owner = open_history();
    owner
        .attach_reference_png(
            owner.content_revision(),
            "asset-r1".to_string(),
            "ref.png".to_string(),
            &oracle_bytes("red-2x1"),
        )
        .unwrap();
    let (revision, depths, saved) = (
        owner.content_revision(),
        owner.history_depths(),
        saved_bytes(&owner),
    );
    let request = attach_request(&owner, "r1", "blue.png", "image/png");
    let response = owner
        .dispatch_asset_command(&request, Some(&oracle_bytes("blue-2x1")))
        .unwrap();
    assert_eq!(
        refusal_of(&response),
        (DispatchErrorCode::BusyConflict, Some("id_collision".into()))
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (revision, depths)
    );
    assert!(records_of(&owner).is_empty());
    assert_eq!(saved_bytes(&owner), saved);
    // Another request id attaches normally, and the project with both still reopens.
    attach_png(&mut owner, "r2", "blue.png", "blue-2x1");
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved_bytes(&owner))
        .expect("the published state is saveable");
    assert_eq!(ids_of(&reopened), ["asset-r2"]);
}

/// A version 2 file holding `count` catalog images (one shared tiny PNG).
fn bulk_catalog_file(count: usize) -> Vec<u8> {
    let b64 = oracle_row("blue-2x1")["b64"].as_str().unwrap().to_string();
    let assets: Vec<Value> = (0..count)
        .map(|i| {
            json!({"id":format!("bulk-{i}"),"name":format!("bulk {i}.png"),"kind":"image",
                "source":{"kind":"embedded","mediaType":"image/png","encoding":"base64",
                    "data":b64}})
        })
        .collect();
    let ids: Vec<String> = (0..count).map(|i| format!("bulk-{i}")).collect();
    let mut file: Value = serde_json::from_slice(&blank_project_file()).unwrap();
    file["formatVersion"] = json!(2);
    file["assets"] = json!(assets);
    file["assetInventory"] = json!({"assetIds":ids,"placements":[]});
    serde_json::to_vec(&file).unwrap()
}

#[test]
fn a_full_catalog_plus_a_separate_viewport_reference_exceeds_the_1024_wire_entries() {
    // Control: the 1024th catalog image alone is saveable (1024 wire entries).
    let mut plain = NativeObjectHistory::from_project_file("desktop", &bulk_catalog_file(1023))
        .expect("1023-asset project");
    attach_png(&mut plain, "last", "rgb.png", "rgb-2x2");
    assert_eq!(records_of(&plain).len(), 1024);
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved_bytes(&plain))
        .expect("a 1024-asset catalog round-trips");
    assert_eq!(records_of(&reopened).len(), 1024);
    // With a reference that is NOT in the catalog the same attach is 1025 entries.
    let mut owner = NativeObjectHistory::from_project_file("desktop", &bulk_catalog_file(1023))
        .expect("1023-asset project");
    owner
        .attach_reference_png(
            owner.content_revision(),
            "plate".to_string(),
            "plate.png".to_string(),
            &oracle_bytes("red-2x1"),
        )
        .unwrap();
    let (revision, depths, saved) = (
        owner.content_revision(),
        owner.history_depths(),
        saved_bytes(&owner),
    );
    let request = attach_request(&owner, "last", "rgb.png", "image/png");
    let response = owner
        .dispatch_asset_command(&request, Some(&oracle_bytes("rgb-2x2")))
        .unwrap();
    assert_eq!(
        refusal_of(&response),
        (UNSAVEABLE.0, Some(UNSAVEABLE.1.to_string()))
    );
    assert_eq!(
        (owner.content_revision(), owner.history_depths()),
        (revision, depths)
    );
    assert_eq!(records_of(&owner).len(), 1023);
    assert_eq!(saved_bytes(&owner), saved);
}

#[test]
fn an_attach_that_would_exceed_the_32_mib_save_envelope_is_refused_and_the_project_still_saves() {
    let mut owner = open_history();
    // Each file is about 8.6 MiB: two fit the 32 MiB envelope (about 23 MiB of base64),
    // the third (about 34 MiB of base64) does not.
    for (index, fill) in [1u8, 2, 3].into_iter().enumerate() {
        let png = solid_png(1500, 1500, fill);
        let request = attach_request(&owner, &format!("big{index}"), "big.png", "image/png");
        let response = owner.dispatch_asset_command(&request, Some(&png)).unwrap();
        if index < 2 {
            assert!(response.is_ok(), "big{index} refused: {response:?}");
            continue;
        }
        let (revision, depths) = (owner.content_revision(), owner.history_depths());
        assert_eq!(
            refusal_of(&response),
            (UNSAVEABLE.0, Some(UNSAVEABLE.1.to_string()))
        );
        assert_eq!(
            (owner.content_revision(), owner.history_depths()),
            (revision, depths)
        );
    }
    assert_eq!(records_of(&owner).len(), 2);
    let saved = saved_bytes(&owner);
    assert!(saved.len() > 20 * 1024 * 1024 && saved.len() <= 32 * 1024 * 1024);
    let reopened = NativeObjectHistory::from_project_file("desktop", &saved)
        .expect("the two published assets reopen");
    assert_eq!(records_of(&reopened).len(), 2);
}

#[test]
fn an_unsaveable_next_project_releases_the_reserved_bytes_and_an_id_collision_never_reserves() {
    let encoded = ObjectSnapshot::encoded_test_budget(1 << 20);
    let cpu = ImageResourceBudget::isolated(8 << 20).unwrap();
    let reserves = Cell::new(0usize);
    let rgb = oracle_bytes("rgb-2x2");
    let prepare = |snapshot: &ObjectSnapshot, request_id: &str| {
        snapshot.prepare_asset_attachment_using(
            request_id,
            "rgb.png",
            "image/png",
            &rgb,
            &cpu,
            |n| {
                reserves.set(reserves.get() + 1);
                encoded.reserve(n).map_err(|_| "reserve")
            },
            |p| p.allocate().map_err(|_| "allocate"),
            |p, a| p.bind(a).map_err(|_| "bind"),
        )
    };
    // 1023 catalog images plus a reference outside the catalog: the 1024th image
    // reserves, fails the dry-run and gives every byte back.
    let mut full = NativeObjectHistory::from_project_file("desktop", &bulk_catalog_file(1023))
        .expect("1023-asset project");
    full.attach_reference_png(
        full.content_revision(),
        "plate".to_string(),
        "plate.png".to_string(),
        &oracle_bytes("red-2x1"),
    )
    .unwrap();
    let snapshot = full.acquire_snapshot(full.content_revision()).unwrap();
    let refusal = prepare(&snapshot, "last").unwrap_err();
    assert_eq!((refusal.code(), refusal.reason()), UNSAVEABLE);
    assert_eq!(reserves.get(), 1);
    assert_eq!(encoded.used(), Ok(0));
    assert_eq!(cpu.used(), Ok(0));
    // The reference id equals the id minted from the request: refused before any
    // reservation.
    let mut owner = open_history();
    owner
        .attach_reference_png(
            owner.content_revision(),
            "asset-c1".to_string(),
            "ref.png".to_string(),
            &oracle_bytes("red-2x1"),
        )
        .unwrap();
    let snapshot = owner.acquire_snapshot(owner.content_revision()).unwrap();
    let refusal = prepare(&snapshot, "c1").unwrap_err();
    assert_eq!(
        (refusal.code(), refusal.reason()),
        (DispatchErrorCode::BusyConflict, "id_collision")
    );
    assert_eq!(reserves.get(), 1);
    assert_eq!(encoded.used(), Ok(0));
}
