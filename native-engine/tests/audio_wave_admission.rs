//! Independent literal byte and retention controls. Authored UNRUN.
use super::*;

// 52 bytes: 2 stereo PCM16 frames at 48000 Hz, alignment 4, byte rate 192000.
// Little-endian sample words: [0, 32767], [-32768, 1]. No encoder/parser helper.
const STEREO: &[u8] = &[
    0x52, 0x49, 0x46, 0x46, 0x2c, 0, 0, 0, 0x57, 0x41, 0x56, 0x45, 0x66, 0x6d, 0x74, 0x20, 0x10, 0,
    0, 0, 1, 0, 2, 0, 0x80, 0xbb, 0, 0, 0, 0xee, 2, 0, 4, 0, 16, 0, 0x64, 0x61, 0x74, 0x61, 8, 0,
    0, 0, 0, 0, 0xff, 0x7f, 0, 0x80, 1, 0,
];

// 48 bytes: 3 mono PCM8 frames at 8000 Hz; the last zero is the RIFF pad,
// not a fourth sample. Data includes unsigned silence (128) and both extremes.
const MONO_ODD: &[u8] = &[
    0x52, 0x49, 0x46, 0x46, 0x28, 0, 0, 0, 0x57, 0x41, 0x56, 0x45, 0x66, 0x6d, 0x74, 0x20, 0x10, 0,
    0, 0, 1, 0, 1, 0, 0x40, 0x1f, 0, 0, 0x40, 0x1f, 0, 0, 1, 0, 8, 0, 0x64, 0x61, 0x74, 0x61, 3, 0,
    0, 0, 0x80, 0, 0xff, 0,
];

fn stereo_metadata() -> WaveMetadata {
    WaveMetadata {
        channels: 2,
        sample_rate: 48000,
        bits_per_sample: 16,
        block_align: 4,
        frame_count: 2,
        duration_seconds: 2.0 / 48000.0,
    }
}

// Fixture composition only; expected metadata is stated independently below.
fn chunk(id: &[u8; 4], body: &[u8]) -> Vec<u8> {
    let mut bytes = id.to_vec();
    bytes.extend_from_slice(&(body.len() as u32).to_le_bytes());
    bytes.extend_from_slice(body);
    if body.len() % 2 != 0 {
        bytes.push(0);
    }
    bytes
}

fn riff(chunks: &[Vec<u8>]) -> Vec<u8> {
    let mut bytes = b"RIFF\0\0\0\0WAVE".to_vec();
    for chunk in chunks {
        bytes.extend_from_slice(chunk);
    }
    let size = (bytes.len() - 8) as u32;
    bytes[4..8].copy_from_slice(&size.to_le_bytes());
    bytes
}

fn pcm_format(tag: u16, bits: u16, channels: u16) -> Vec<u8> {
    let mut bytes = STEREO[20..36].to_vec();
    bytes[0..2].copy_from_slice(&tag.to_le_bytes());
    bytes[2..4].copy_from_slice(&channels.to_le_bytes());
    let align = channels * (bits / 8);
    bytes[8..12].copy_from_slice(&(48000u32 * u32::from(align)).to_le_bytes());
    bytes[12..14].copy_from_slice(&align.to_le_bytes());
    bytes[14..16].copy_from_slice(&bits.to_le_bytes());
    bytes
}

fn from_format(format: &[u8], data: &[u8]) -> Vec<u8> {
    riff(&[chunk(b"fmt ", format), chunk(b"data", data)])
}

fn validate(bytes: &[u8]) -> Result<WaveMetadata, WaveError> {
    validate_wave(bytes, usize::MAX)
}

#[test]
fn literal_stereo_words_produce_exact_frame_count_duration_and_format() {
    assert_eq!(STEREO.len(), 52);
    assert_eq!(validate(STEREO), Ok(stereo_metadata()));
}

#[test]
fn literal_odd_data_pad_is_not_counted_as_a_frame() {
    assert_eq!(MONO_ODD.len(), 48);
    assert_eq!(
        validate(MONO_ODD),
        Ok(WaveMetadata {
            channels: 1,
            sample_rate: 8000,
            bits_per_sample: 8,
            block_align: 1,
            frame_count: 3,
            duration_seconds: 3.0 / 8000.0,
        })
    );
}

#[test]
fn requested_documented_pcm_widths_and_ieee_float32_are_container_values_only() {
    for (tag, bits) in [(1, 8), (1, 16), (1, 24), (1, 32), (3, 32)] {
        for channels in [1, 2] {
            let fmt = pcm_format(tag, bits, channels);
            let frame_bytes = usize::from(channels) * usize::from(bits / 8);
            let bytes = from_format(&fmt, &vec![0; frame_bytes * 4]);
            let result = validate(&bytes).unwrap();
            assert_eq!(result.channels, channels);
            assert_eq!(result.bits_per_sample, bits);
            assert_eq!(result.sample_rate, 48000);
            assert_eq!(result.block_align, frame_bytes as u16);
            assert_eq!(result.frame_count, 4);
            assert_eq!(result.duration_seconds, 4.0 / 48000.0);
        }
    }
}

#[test]
fn empty_data_has_zero_duration_without_claiming_an_audible_track() {
    let bytes = from_format(&STEREO[20..36], &[]);
    let result = validate(&bytes).unwrap();
    assert_eq!(result.frame_count, 0);
    assert_eq!(result.duration_seconds, 0.0);
}

#[test]
fn exact_byte_limit_is_inclusive_and_zero_or_smaller_limit_refuses() {
    assert_eq!(validate_wave(STEREO, STEREO.len()), Ok(stereo_metadata()));
    assert_eq!(
        validate_wave(STEREO, STEREO.len() - 1),
        Err(WaveError::Limit)
    );
    assert_eq!(validate_wave(STEREO, 0), Err(WaveError::Limit));
}

#[test]
fn every_truncated_literal_prefix_and_repaired_outer_size_is_invalid() {
    for end in 0..STEREO.len() {
        assert_eq!(
            validate(&STEREO[..end]),
            Err(WaveError::Invalid),
            "prefix {end}"
        );
        if end >= 12 {
            let mut repaired = STEREO[..end].to_vec();
            repaired[4..8].copy_from_slice(&((end - 8) as u32).to_le_bytes());
            assert_eq!(
                validate(&repaired),
                Err(WaveError::Invalid),
                "repaired prefix {end}"
            );
        }
    }
}

#[test]
fn trailing_bytes_and_forged_outer_or_chunk_lengths_never_hide_truncation() {
    let mut extra = STEREO.to_vec();
    extra.push(0);
    assert_eq!(validate(&extra), Err(WaveError::Invalid));
    for at in [4, 16, 40] {
        let mut forged = STEREO.to_vec();
        forged[at..at + 4].copy_from_slice(&u32::MAX.to_le_bytes());
        assert_eq!(validate(&forged), Err(WaveError::Invalid));
    }
    let mut trailing_header = STEREO.to_vec();
    trailing_header.extend_from_slice(b"JUNK");
    trailing_header[4..8].copy_from_slice(&48u32.to_le_bytes());
    assert_eq!(validate(&trailing_header), Err(WaveError::Invalid));
}

#[test]
fn odd_chunk_padding_must_exist_and_be_zero() {
    let mut nonzero = MONO_ODD.to_vec();
    *nonzero.last_mut().unwrap() = 1;
    assert_eq!(validate(&nonzero), Err(WaveError::Invalid));
    let mut missing = MONO_ODD[..47].to_vec();
    missing[4..8].copy_from_slice(&39u32.to_le_bytes());
    assert_eq!(validate(&missing), Err(WaveError::Invalid));
}

#[test]
fn one_required_format_must_precede_one_required_data() {
    let fmt = chunk(b"fmt ", &STEREO[20..36]);
    let data = chunk(b"data", &STEREO[44..]);
    for chunks in [
        vec![],
        vec![fmt.clone()],
        vec![data.clone()],
        vec![data.clone(), fmt.clone()],
        vec![fmt.clone(), fmt.clone(), data.clone()],
        vec![fmt, data.clone(), data],
    ] {
        assert_eq!(validate(&riff(&chunks)), Err(WaveError::Invalid));
    }
}

#[test]
fn ancillary_chunk_is_skipped_but_its_own_boundary_and_pad_are_checked() {
    let chunks = [
        chunk(b"fmt ", &STEREO[20..36]),
        chunk(b"JUNK", b"x"),
        chunk(b"data", &STEREO[44..]),
        chunk(b"note", b"done"),
    ];
    let mut bytes = riff(&chunks);
    assert_eq!(validate(&bytes), Ok(stereo_metadata()));
    bytes[45] = 7; // JUNK pad: 12 + 24 + 8-byte header + one payload byte.
    assert_eq!(validate(&bytes), Err(WaveError::Invalid));
}

#[test]
fn legacy_and_zero_extension_fmt_are_supported_but_extensions_are_not_assumed() {
    let mut extended = STEREO[20..36].to_vec();
    extended.extend_from_slice(&[0, 0]);
    assert_eq!(
        validate(&from_format(&extended, &STEREO[44..])),
        Ok(stereo_metadata())
    );
    extended.pop();
    assert_eq!(
        validate(&from_format(&extended, &STEREO[44..])),
        Err(WaveError::Invalid)
    );
    extended.extend_from_slice(&[0, 0]); // 19 bytes with cbSize zero is inconsistent.
    assert_eq!(
        validate(&from_format(&extended, &STEREO[44..])),
        Err(WaveError::Invalid)
    );
    let mut unsupported = STEREO[20..36].to_vec();
    unsupported.extend_from_slice(&[2, 0, 0, 0]);
    assert_eq!(
        validate(&from_format(&unsupported, &STEREO[44..])),
        Err(WaveError::Unsupported)
    );
}

#[test]
fn wrong_container_or_codec_is_not_accepted_from_magic_or_mime() {
    for magic in [b"RIFX", b"RF64", b"BW64"] {
        let mut bytes = STEREO.to_vec();
        bytes[..4].copy_from_slice(magic);
        assert_eq!(validate(&bytes), Err(WaveError::Unsupported));
    }
    let mut bytes = STEREO.to_vec();
    bytes[..4].copy_from_slice(b"ID3\0");
    assert_eq!(validate(&bytes), Err(WaveError::Invalid));
    bytes[..4].copy_from_slice(b"RIFF");
    bytes[8..12].copy_from_slice(b"AVI ");
    assert_eq!(validate(&bytes), Err(WaveError::Unsupported));
    for (tag, bits) in [
        (2, 16),
        (6, 8),
        (7, 8),
        (0xfffe, 16),
        (1, 12),
        (3, 16),
        (3, 64),
    ] {
        let bytes = from_format(&pcm_format(tag, bits, 2), &[0; 16]);
        assert_eq!(
            validate(&bytes),
            Err(WaveError::Unsupported),
            "tag {tag} bits {bits}"
        );
    }
}

#[test]
fn zero_rate_channel_and_bad_alignment_byte_rate_or_overflow_are_invalid() {
    for (at, value) in [(22, 0u16), (32, 0), (32, 3)] {
        let mut bytes = STEREO.to_vec();
        bytes[at..at + 2].copy_from_slice(&value.to_le_bytes());
        assert_eq!(validate(&bytes), Err(WaveError::Invalid));
    }
    for (at, value) in [(24, 0u32), (28, 0), (28, 1), (24, u32::MAX)] {
        let mut bytes = STEREO.to_vec();
        bytes[at..at + 4].copy_from_slice(&value.to_le_bytes());
        assert_eq!(validate(&bytes), Err(WaveError::Invalid));
    }
    assert_eq!(
        validate(&from_format(&pcm_format(1, 16, 3), &[0; 12])),
        Err(WaveError::Unsupported)
    );
}

#[test]
fn partial_frames_and_alternative_segmented_audio_are_refused() {
    assert_eq!(
        validate(&from_format(&STEREO[20..36], &[0; 7])),
        Err(WaveError::Invalid)
    );
    let bytes = riff(&[chunk(b"fmt ", &STEREO[20..36]), chunk(b"LIST", b"wavl")]);
    assert_eq!(validate(&bytes), Err(WaveError::Unsupported));
}

#[test]
fn admission_retains_exact_arc_and_metadata_without_copying_or_changing_bytes() {
    let allocation = Arc::new(STEREO.to_vec());
    let admitted = admit_wave(allocation.clone(), allocation.capacity()).unwrap();
    assert!(Arc::ptr_eq(admitted.allocation(), &allocation));
    assert_eq!(admitted.allocation().as_slice(), STEREO);
    assert_eq!(admitted.metadata(), stereo_metadata());
    assert_eq!(Arc::strong_count(&allocation), 2);
}

#[test]
fn retained_capacity_is_bounded_even_when_valid_used_bytes_fit() {
    let mut bytes = Vec::with_capacity(STEREO.len() + 128);
    bytes.extend_from_slice(STEREO);
    let allocation = Arc::new(bytes);
    assert!(allocation.capacity() > allocation.len());
    assert_eq!(
        admit_wave(allocation.clone(), allocation.len()).unwrap_err(),
        WaveError::Limit
    );
    assert_eq!(Arc::strong_count(&allocation), 1);
    let admitted = admit_wave(allocation.clone(), allocation.capacity()).unwrap();
    assert!(Arc::ptr_eq(admitted.allocation(), &allocation));
    assert_eq!(
        admit_wave(allocation.clone(), 0).unwrap_err(),
        WaveError::Limit
    );
}

#[test]
fn invalid_admission_retains_nothing_and_cloned_admission_pins_until_last_drop() {
    let invalid = Arc::new(vec![0; 52]);
    assert_eq!(
        admit_wave(invalid.clone(), invalid.capacity()).unwrap_err(),
        WaveError::Invalid
    );
    assert_eq!(Arc::strong_count(&invalid), 1);
    let original = Arc::new(STEREO.to_vec());
    let weak = Arc::downgrade(&original);
    let admitted = admit_wave(original.clone(), original.capacity()).unwrap();
    let snapshot_pin = admitted.clone();
    drop(original);
    drop(admitted);
    assert!(weak.upgrade().is_some());
    assert_eq!(snapshot_pin.metadata(), stereo_metadata());
    drop(snapshot_pin);
    assert!(weak.upgrade().is_none());
}

#[test]
fn external_copy_on_write_cannot_change_the_admitted_allocation() {
    let mut external = Arc::new(STEREO.to_vec());
    let admitted = admit_wave(external.clone(), external.capacity()).unwrap();
    Arc::make_mut(&mut external)[44] = 99;
    assert!(!Arc::ptr_eq(&external, admitted.allocation()));
    assert_eq!(admitted.allocation().as_slice(), STEREO);
    assert_eq!(admitted.metadata(), stereo_metadata());
}
