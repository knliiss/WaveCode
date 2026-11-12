//! Independent experimental proofs for adaptive domain rank packing.
//! These tests do not modify or invoke the released V1/V2 wire formats.

const BASE62: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const BASE64URL: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const APPLE_STOREFRONTS: u16 = 26 * 26;
const NUMERIC_SERVICES: u16 = 4;
const RANK_GROUPS: u16 = APPLE_STOREFRONTS + NUMERIC_SERVICES;

fn spotify_rank(id: &str) -> Option<[u8; 17]> {
    if id.len() != 22 {
        return None;
    }
    let mut number = [0_u8; 17];
    for character in id.bytes() {
        let mut carry = BASE62.iter().position(|&digit| digit == character)? as u16;
        for byte in number.iter_mut().rev() {
            let next = u16::from(*byte) * 62 + carry;
            *byte = next as u8;
            carry = next >> 8;
        }
        if carry != 0 {
            return None;
        }
    }
    Some(number)
}

fn spotify_from_rank(mut rank: [u8; 17]) -> Option<String> {
    let mut digits = [0_u8; 22];
    for digit in digits.iter_mut().rev() {
        let mut remainder = 0_u16;
        for byte in &mut rank {
            let numerator = (remainder << 8) | u16::from(*byte);
            *byte = (numerator / 62) as u8;
            remainder = numerator % 62;
        }
        *digit = BASE62[remainder as usize];
    }
    (rank.iter().all(|&value| value == 0))
        .then(|| String::from_utf8(digits.to_vec()).expect("ASCII Base62"))
}

fn bit_length(bytes: &[u8]) -> usize {
    bytes
        .iter()
        .enumerate()
        .find(|(_, byte)| **byte != 0)
        .map_or(0, |(index, byte)| {
            (bytes.len() - 1 - index) * 8 + (8 - byte.leading_zeros()) as usize
        })
}

fn youtube_rank(id: &str) -> Option<u64> {
    if id.len() != 11 {
        return None;
    }
    let mut word = 0_u128;
    for character in id.bytes() {
        let digit = BASE64URL.iter().position(|&value| value == character)? as u128;
        word = (word << 6) | digit;
    }
    if word & 3 != 0 {
        return None;
    }
    Some((word >> 2) as u64)
}

fn youtube_from_rank(id: u64) -> String {
    let mut number = u128::from(id) << 2;
    let mut result = [0_u8; 11];
    for byte in result.iter_mut().rev() {
        *byte = BASE64URL[(number & 63) as usize];
        number >>= 6;
    }
    String::from_utf8(result.to_vec()).expect("ASCII Base64URL")
}

fn combined_rank(group: u16, id: u64) -> Option<u128> {
    (group < RANK_GROUPS).then_some((u128::from(group) << 64) | u128::from(id))
}

fn combined_unrank(rank: u128) -> Option<(u16, u64)> {
    let group = rank >> 64;
    (group < u128::from(RANK_GROUPS)).then_some((group as u16, rank as u64))
}

fn columns(info_bits: usize) -> usize {
    (info_bits + 16).div_ceil(8) + 4
}

fn next(seed: &mut u64) -> u64 {
    *seed ^= *seed << 13;
    *seed ^= *seed >> 7;
    *seed ^= *seed << 17;
    *seed
}

#[test]
fn official_spotify_id_requires_more_than_128_bits() {
    let example = "7ouMYWpwJ422jRcDASZB7P";
    let number = spotify_rank(example).unwrap();
    assert_eq!(bit_length(&number), 129);
    assert_eq!(spotify_from_rank(number).as_deref(), Some(example));
    assert_eq!(bit_length(&spotify_rank(&"z".repeat(22)).unwrap()), 131);
}

#[test]
fn base62_rank_roundtrips_all_edge_classes_and_random_ids() {
    let edge_cases = [
        "0000000000000000000000",
        "0VjIjW4GlUZAMYd2vXMi3b",
        "6rqhFgbbKwnb9MLmUQDhG6",
        "7ouMYWpwJ422jRcDASZB7P",
        "zzzzzzzzzzzzzzzzzzzzzz",
    ];
    for id in edge_cases {
        let number = spotify_rank(id).unwrap();
        assert_eq!(spotify_from_rank(number).as_deref(), Some(id));
    }

    let mut seed = 0xd846d9271a135be5_u64;
    for _ in 0..10_000 {
        let id: String = (0..22)
            .map(|_| BASE62[(next(&mut seed) % 62) as usize] as char)
            .collect();
        assert_eq!(spotify_from_rank(spotify_rank(&id).unwrap()).unwrap(), id);
    }
}

#[test]
fn canonical_youtube_video_ids_are_exactly_64_bit_ranks() {
    assert_eq!(youtube_rank("JGwWNGJdvx8"), Some(0x246c1634625dbf1f));
    for id in [0, 1, u64::MAX, 0x123456789abcdef0] {
        assert_eq!(youtube_rank(&youtube_from_rank(id)), Some(id));
    }
    assert!(youtube_rank("JGwWNGJdvx9").is_none());
    let mut seed = 0x659b41df7a319763;
    for _ in 0..10_000 {
        let id = next(&mut seed);
        assert_eq!(youtube_rank(&youtube_from_rank(id)), Some(id));
    }
}

#[test]
fn leftover_apple_country_slots_fit_four_numeric_namespaces() {
    assert_eq!(APPLE_STOREFRONTS, 676);
    assert_eq!(RANK_GROUPS, 680);
    assert!(RANK_GROUPS <= 1 << 10);
    assert_eq!(1024 - RANK_GROUPS, 344);
    for group in 0..RANK_GROUPS {
        for id in [0, 1, 0x123456789abcdef0, u64::MAX] {
            let combined = combined_rank(group, id).unwrap();
            assert_eq!(combined_unrank(combined), Some((group, id)));
            assert!(combined < (1_u128 << 74));
        }
    }
    assert_eq!(
        combined_unrank(combined_rank(675, u64::MAX).unwrap()),
        Some((675, u64::MAX))
    );
    assert_eq!(
        combined_unrank(combined_rank(676, 0).unwrap()),
        Some((676, 0))
    );
    assert_eq!(combined_unrank(680_u128 << 64), None);
    assert_eq!(combined_rank(680, 0), None);

    let mut seed = 0x6d0dbf2ee37c4a63;
    for _ in 0..20_000 {
        let group = (next(&mut seed) % u64::from(RANK_GROUPS)) as u16;
        let id = next(&mut seed);
        assert_eq!(
            combined_unrank(combined_rank(group, id).unwrap()),
            Some((group, id))
        );
    }
}

#[test]
fn proposed_column_budgets_preserve_crc16_and_four_rs_bytes() {
    assert_eq!(columns(4 + 66), 15);
    assert_eq!(columns(64), 14);
    assert_eq!(columns(64 + 8), 15);
    assert_eq!(columns(131), 23);
    assert_eq!(columns(128), 22);
    assert_eq!(columns(131 + 8), 24);
    assert_eq!(columns(128 + 8), 23);
    assert_eq!(columns(131 + 66), 31);
    assert_eq!(columns(131 + 64), 31);
    assert_eq!(columns(131 + 64 + 8), 32);
    assert_eq!(columns(128 + 64), 30);
    assert_eq!(columns(128 + 64 + 8), 31);
    assert_eq!(columns(2 + 32), 11);
    assert_eq!(columns(74), 16);
}
