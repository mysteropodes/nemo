//! The staged-bytes verifier against REAL encoder output: the repository's PNG
//! encoder, baseline-shaped SVG documents and an independent PNG decoder.
use super::stage::{
    assemble, chunk, crc32_bitwise, decoded_header, ihdr, owned, plan, png_bytes, staged, svg_text,
};
use super::*;

#[test]
fn the_production_crc_agrees_with_the_standard_check_value_and_the_bitwise_one() {
    assert_eq!(crc32_bitwise(b"123456789"), 0xCBF4_3926);
    let table = crc_table();
    assert_eq!(crc32(&table, b"123456789"), 0xCBF4_3926);
    assert_eq!(crc32(&table, b""), 0);
    let sample: Vec<u8> = (0..=255u8).cycle().take(5000).collect();
    assert_eq!(crc32(&table, &sample), crc32_bitwise(&sample));
}

#[test]
fn real_encoder_pngs_verify_and_agree_with_an_independent_decoder() {
    let plan = plan(FrameFormat::Png, 10, 12, 64, 48);
    let batch = plan.batch(0).unwrap();
    let files = owned(&batch.staged_names, |i| png_bytes(64, 48, i as u8));
    for file in &files {
        assert_eq!(decoded_header(&file.bytes), (64, 48, true, true));
    }
    let verified = verify_batch(&plan, 0, NameSpace::Staged, &staged(&files)).unwrap();
    let names: Vec<&str> = verified.files.iter().map(|f| f.name.as_str()).collect();
    assert_eq!(
        names,
        vec!["frame_0001.png", "frame_0002.png", "frame_0003.png"]
    );
    for (done, file) in verified.files.iter().zip(&files) {
        assert_eq!((done.width, done.height), (64, 48));
        assert_eq!(done.png_format, Some((8, 6)));
        assert_eq!(done.byte_len, file.bytes.len());
    }
    let sum: usize = files.iter().map(|f| f.bytes.len()).sum();
    assert_eq!(verified.total_bytes, sum as u64);
    assert_ne!(
        files[0].bytes, files[1].bytes,
        "frames differ, so the check is not trivially uniform"
    );
}

#[test]
fn a_multi_batch_output_verifies_staged_names_per_batch_and_final_names_whole() {
    let plan = plan(FrameFormat::Png, 0, 69, 4, 3);
    let tiny = |i: usize| png_bytes(4, 3, i as u8);
    for index in 0..plan.batch_count() {
        let batch = plan.batch(index).unwrap();
        let files = owned(&batch.staged_names, tiny);
        let ok = verify_batch(&plan, index, NameSpace::Staged, &staged(&files)).unwrap();
        assert_eq!(ok.files.len(), batch.frames.len());
    }
    // Batch 1 staged files carry frame_0001.. again; as final names they are wrong.
    let second = plan.batch(1).unwrap();
    let restarted = owned(&second.staged_names, tiny);
    assert_eq!(
        verify_batch(&plan, 1, NameSpace::Final, &staged(&restarted)),
        Err(VerifyFailure::NameMismatch {
            index: 0,
            expected: "frame_0033.png".into(),
            actual: "frame_0001.png".into(),
        })
    );
    let renamed = owned(&second.final_names, tiny);
    assert!(verify_batch(&plan, 1, NameSpace::Final, &staged(&renamed)).is_ok());
    // The merged output: all 70 files under their final names.
    let mut all_names = Vec::new();
    for index in 0..plan.batch_count() {
        all_names.extend(plan.batch(index).unwrap().final_names);
    }
    let all = owned(&all_names, tiny);
    let whole = verify_output(&plan, &staged(&all)).unwrap();
    assert_eq!(whole.files.len(), 70);
    assert_eq!(whole.files.last().unwrap().name, "frame_0070.png");
    assert_eq!(
        verify_batch(&plan, 3, NameSpace::Final, &[]),
        Err(VerifyFailure::UnknownBatch { index: 3 })
    );
}

#[test]
fn svg_frames_verify_against_the_pinned_dimensions() {
    let plan = plan(FrameFormat::Svg, 0, 1, 320, 180);
    let files = owned(&plan.batch(0).unwrap().staged_names, |_| {
        svg_text(320, 180).into_bytes()
    });
    let verified = verify_output(&plan, &staged(&files)).unwrap();
    assert_eq!(verified.files.len(), 2);
    assert_eq!(
        (
            verified.files[0].width,
            verified.files[0].height,
            verified.files[0].png_format
        ),
        (320, 180, None)
    );
    let wrong = owned(&plan.batch(0).unwrap().staged_names, |_| {
        svg_text(321, 180).into_bytes()
    });
    assert_eq!(
        verify_output(&plan, &staged(&wrong)),
        Err(VerifyFailure::DimensionMismatch {
            name: "frame_0001.svg".into(),
            expected: (320, 180),
            actual: (321, 180),
        })
    );
}

#[test]
fn dimensions_come_from_the_bytes_not_from_the_name_or_the_plan_of_another_size() {
    let plan = plan(FrameFormat::Png, 0, 0, 64, 48);
    let transposed = owned(&plan.batch(0).unwrap().staged_names, |_| {
        png_bytes(48, 64, 0)
    });
    assert_eq!(
        verify_output(&plan, &staged(&transposed)),
        Err(VerifyFailure::DimensionMismatch {
            name: "frame_0001.png".into(),
            expected: (64, 48),
            actual: (48, 64),
        })
    );
}

#[test]
fn count_name_and_order_failures_are_typed() {
    let plan = plan(FrameFormat::Png, 0, 2, 8, 8);
    let names = plan.batch(0).unwrap().staged_names;
    let files = owned(&names, |i| png_bytes(8, 8, i as u8));
    let all = staged(&files);
    assert_eq!(
        verify_output(&plan, &all[..2]),
        Err(VerifyFailure::CountMismatch {
            expected: 3,
            actual: 2
        })
    );
    let mut extra = all.clone();
    extra.push(all[0]);
    assert_eq!(
        verify_output(&plan, &extra),
        Err(VerifyFailure::CountMismatch {
            expected: 3,
            actual: 4
        })
    );
    let swapped = vec![all[0], all[2], all[1]];
    assert_eq!(
        verify_output(&plan, &swapped),
        Err(VerifyFailure::OutOfOrder { index: 2 })
    );
    let duplicated = vec![all[0], all[1], all[1]];
    assert_eq!(
        verify_output(&plan, &duplicated),
        Err(VerifyFailure::OutOfOrder { index: 2 })
    );
    let gap = vec![
        all[0],
        all[1],
        StagedFile {
            name: "frame_0004.png",
            bytes: all[2].bytes,
        },
    ];
    assert_eq!(
        verify_output(&plan, &gap),
        Err(VerifyFailure::NameMismatch {
            index: 2,
            expected: "frame_0003.png".into(),
            actual: "frame_0004.png".into(),
        })
    );
    let foreign = vec![
        all[0],
        all[1],
        StagedFile {
            name: "frame_0003.jpg",
            bytes: all[2].bytes,
        },
    ];
    assert!(matches!(
        verify_output(&plan, &foreign),
        Err(VerifyFailure::NameMismatch { index: 2, .. })
    ));
}

#[test]
fn empty_and_truncated_files_are_typed_malformed() {
    let plan = plan(FrameFormat::Png, 0, 0, 16, 16);
    let name = plan.batch(0).unwrap().staged_names;
    let good = png_bytes(16, 16, 3);
    let cases: Vec<(Vec<u8>, VerifyFailure)> = vec![
        (
            Vec::new(),
            VerifyFailure::EmptyFile {
                name: "frame_0001.png".into(),
            },
        ),
        (
            good[..good.len() - 12].to_vec(),
            VerifyFailure::Malformed {
                name: "frame_0001.png".into(),
                reason: "missing IEND chunk",
            },
        ),
        (
            good[..4].to_vec(),
            VerifyFailure::Malformed {
                name: "frame_0001.png".into(),
                reason: "missing PNG signature",
            },
        ),
        (
            assemble(&[ihdr(16, 16), chunk(b"IEND", &[])]),
            VerifyFailure::Malformed {
                name: "frame_0001.png".into(),
                reason: "no IDAT before IEND",
            },
        ),
    ];
    for (bytes, expected) in cases {
        let files = owned(&name, |_| bytes.clone());
        assert_eq!(verify_output(&plan, &staged(&files)), Err(expected));
    }
}
