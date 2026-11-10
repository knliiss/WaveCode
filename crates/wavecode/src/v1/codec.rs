use crate::identity;
use crate::rank::Rank;
use crate::{Decoded, Error, MusicCode};

use super::rs::{self, DATA_SYMBOLS};

pub const FORMAT_VERSION: u8 = 1;
pub const SYMBOL_COUNT: usize = rs::TOTAL_SYMBOLS;
pub const DATA_COUNT: usize = DATA_SYMBOLS;
pub const CORRECTION_SYMBOLS: usize = rs::PARITY_SYMBOLS;

const MESSAGE_BYTES: usize = 19;
const PACKED_BYTES: usize = 21;
const VERSION_SHIFT: u8 = 5;
const MODE_SHIFT: u8 = 3;
const MODE_TRACK: u8 = 0;
const MODE_MOMENT: u8 = 1;

fn checksum(bytes: &[u8]) -> u16 {
    let mut crc = 0xffff_u16;
    for &byte in bytes {
        crc ^= u16::from(byte) << 8;
        for _ in 0..8 {
            crc = if crc & 0x8000 != 0 {
                (crc << 1) ^ 0x1021
            } else {
                crc << 1
            };
        }
    }
    crc
}

fn whitening(data: &mut [u8; DATA_SYMBOLS]) {
    let mut mask = 0x25_u8;
    for symbol in data {
        let overflow = mask & 0x20 != 0;
        mask = (mask << 1) & 0x3f;
        if overflow {
            mask ^= 0x03;
        }
        *symbol ^= mask;
    }
}

fn to_sextets(bytes: &[u8; PACKED_BYTES]) -> [u8; DATA_SYMBOLS] {
    std::array::from_fn(|symbol| {
        let mut result = 0_u8;
        for bit in 0..6 {
            let index = symbol * 6 + bit;
            result = (result << 1) | ((bytes[index / 8] >> (7 - index % 8)) & 1);
        }
        result
    })
}

fn from_sextets(data: &[u8; DATA_SYMBOLS]) -> [u8; PACKED_BYTES] {
    let mut result = [0_u8; PACKED_BYTES];
    for (symbol_index, symbol) in data.iter().enumerate() {
        for bit in 0..6 {
            let index = symbol_index * 6 + bit;
            let value = (symbol >> (5 - bit)) & 1;
            result[index / 8] |= value << (7 - index % 8);
        }
    }
    result
}

pub fn encode(code: &MusicCode) -> Result<Vec<u8>, Error> {
    let rank = identity::encode(&code.track)?;
    let mut bytes = [0_u8; PACKED_BYTES];
    let rank_bytes = rank.to_bytes(17).ok_or(Error::InvalidCodeword)?;
    if rank_bytes[0] > 7 {
        return Err(Error::InvalidCodeword);
    }
    bytes[..17].copy_from_slice(&rank_bytes);

    let mode = if code.moment_seconds.is_some() {
        MODE_MOMENT
    } else {
        MODE_TRACK
    };
    bytes[0] |= (FORMAT_VERSION << VERSION_SHIFT) | (mode << MODE_SHIFT);
    bytes[17..MESSAGE_BYTES].copy_from_slice(&code.moment_seconds.unwrap_or(0).to_be_bytes());

    let crc = checksum(&bytes[..MESSAGE_BYTES]);
    bytes[MESSAGE_BYTES..].copy_from_slice(&crc.to_be_bytes());

    let mut data = to_sextets(&bytes);
    whitening(&mut data);
    Ok(rs::encode(&data).to_vec())
}

pub fn decode(symbols: &[u8]) -> Result<Decoded, Error> {
    decode_with_erasures(symbols, &[])
}

pub fn decode_with_erasures(symbols: &[u8], erasures: &[usize]) -> Result<Decoded, Error> {
    let (recovered, corrected_symbols) = rs::decode(symbols, erasures)?;
    let mut data = [0_u8; DATA_SYMBOLS];
    data.copy_from_slice(&recovered[..DATA_SYMBOLS]);
    whitening(&mut data);

    let bytes = from_sextets(&data);
    let actual = u16::from_be_bytes([bytes[MESSAGE_BYTES], bytes[MESSAGE_BYTES + 1]]);
    if actual != checksum(&bytes[..MESSAGE_BYTES]) {
        return Err(Error::ChecksumMismatch);
    }

    let version = bytes[0] >> VERSION_SHIFT;
    if version != FORMAT_VERSION {
        return Err(Error::UnsupportedVersion);
    }

    let mode = (bytes[0] >> MODE_SHIFT) & 0x03;
    let mut rank_bytes = [0_u8; 17];
    rank_bytes.copy_from_slice(&bytes[..17]);
    rank_bytes[0] &= 0x07;
    let rank = Rank::from_bytes(&rank_bytes);

    if rank >= identity::capacity() {
        return Err(Error::InvalidCodeword);
    }
    let seconds = u16::from_be_bytes([bytes[17], bytes[18]]);
    let moment_seconds = match mode {
        MODE_TRACK if seconds == 0 => None,
        MODE_MOMENT => Some(seconds),
        _ => return Err(Error::InvalidCodeword),
    };

    Ok(Decoded {
        code: MusicCode {
            track: identity::decode(rank)?,
            moment_seconds,
        },
        corrected_symbols,
    })
}

#[cfg(test)]
mod tests {
    use super::{
        checksum, decode, encode, from_sextets, to_sextets, whitening, FORMAT_VERSION, PACKED_BYTES,
    };
    use crate::{Error, MusicCode, Track};

    #[test]
    fn checksum_matches_standard_reference() {
        assert_eq!(checksum(b"123456789"), 0x29b1);
    }

    #[test]
    fn bit_packing_is_bijective() {
        let sample = std::array::from_fn(|i| (i as u8).wrapping_mul(37));
        let symbols = to_sextets(&sample);
        assert!(symbols.iter().all(|value| *value < 64));
        assert_eq!(from_sextets(&symbols), sample);
        let empty = [0_u8; PACKED_BYTES];
        assert_eq!(from_sextets(&to_sextets(&empty)), empty);
    }

    #[test]
    fn whitening_is_reversible() {
        let mut sample = [0_u8; 28];
        whitening(&mut sample);
        assert!(sample.iter().any(|value| *value != 0));
        whitening(&mut sample);
        assert_eq!(sample, [0_u8; 28]);
    }

    #[test]
    fn versioned_round_trips_for_all_platforms() {
        assert_eq!(FORMAT_VERSION, 1);
        let tracks = [
            Track::YouTubeMusic("AAAAAAAAAAA".into()),
            Track::YouTubeMusic("___________".into()),
            Track::Spotify("0000000000000000000000".into()),
            Track::Spotify("zzzzzzzzzzzzzzzzzzzzzz".into()),
            Track::AppleMusic {
                id: 0,
                storefront: "aa".into(),
            },
            Track::AppleMusic {
                id: u64::MAX,
                storefront: "zz".into(),
            },
        ];

        for track in tracks {
            for moment_seconds in [None, Some(0), Some(187), Some(4095), Some(4096), Some(65535)] {
                let code = MusicCode {
                    track: track.clone(),
                    moment_seconds,
                };
                let symbols = encode(&code).unwrap();
                assert_eq!(symbols.len(), 34);
                assert!(symbols.iter().all(|value| *value < 64));
                assert_eq!(decode(&symbols).unwrap().code, code);
            }
        }
    }

    #[test]
    fn crc_rejects_a_valid_rs_word_with_modified_payload() {
        let code = MusicCode {
            track: Track::YouTubeMusic("WYYkdveJyn8".into()),
            moment_seconds: Some(187),
        };
        let original = encode(&code).unwrap();

        let mut message = [0_u8; 28];
        message.copy_from_slice(&original[..28]);
        whitening(&mut message);
        let mut bytes = from_sextets(&message);
        bytes[12] ^= 0x10;
        message = to_sextets(&bytes);
        whitening(&mut message);
        let spoofed = super::rs::encode(&message);
        assert_eq!(decode(&spoofed), Err(Error::ChecksumMismatch));
    }
}
