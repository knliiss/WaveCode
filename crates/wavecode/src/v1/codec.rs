use crate::identity;
use crate::rank::Rank;
use crate::{Decoded, Error, MusicCode};

use super::rs::{self, DATA_SYMBOLS};

pub const FORMAT_VERSION: u8 = 1;
pub const SYMBOL_COUNT: usize = rs::TOTAL_SYMBOLS;
pub const DATA_COUNT: usize = DATA_SYMBOLS;
pub const CORRECTION_SYMBOLS: usize = rs::PARITY_SYMBOLS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Observation {
    pub symbol: u8,
    pub confidence: u8,
}

pub fn decode_observations(observations: &[Observation]) -> Result<Decoded, Error> {
    if observations.len() != SYMBOL_COUNT {
        return Err(Error::InvalidLength);
    }
    if observations
        .iter()
        .any(|observation| observation.symbol >= 64)
    {
        return Err(Error::InvalidCodeword);
    }

    let symbols: Vec<u8> = observations.iter().map(|item| item.symbol).collect();
    let mut priorities: Vec<usize> = (0..SYMBOL_COUNT).collect();
    priorities.sort_by_key(|&index| (observations[index].confidence, index));

    for count in 0..=CORRECTION_SYMBOLS {
        match decode_with_erasures(&symbols, &priorities[..count]) {
            Ok(decoded) => return Ok(decoded),
            Err(Error::ChecksumMismatch | Error::Uncorrectable | Error::InvalidCodeword) => {}
            Err(error) => return Err(error),
        }
    }

    Err(Error::Uncorrectable)
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct SymbolHypothesis {
    pub symbol: u8,
    pub confidence: u8,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct SoftObservation {
    pub candidates: Vec<SymbolHypothesis>,
}

fn try_soft(symbols: &[u8], fixed_erasures: &[usize], confidences: &[u8]) -> Option<Decoded> {
    if let Ok(decoded) = decode_with_erasures(symbols, fixed_erasures) {
        return Some(decoded);
    }

    let mut ranking: Vec<usize> = (0..SYMBOL_COUNT)
        .filter(|index| !fixed_erasures.contains(index))
        .collect();
    ranking.sort_by_key(|&index| (confidences[index], index));

    for extra in 1..=CORRECTION_SYMBOLS - fixed_erasures.len() {
        let mut erasures = fixed_erasures.to_vec();
        erasures.extend_from_slice(&ranking[..extra]);
        if let Ok(decoded) = decode_with_erasures(symbols, &erasures) {
            return Some(decoded);
        }
    }
    None
}

struct SoftSearch<'a> {
    observations: &'a [SoftObservation],
    fixed_erasures: &'a [usize],
    confidences: &'a [u8],
    uncertain: &'a [usize],
    attempts: usize,
}

impl SoftSearch<'_> {
    fn search(&mut self, symbols: &mut [u8], depth: usize, changed: bool) -> Option<Decoded> {
        if self.attempts >= 512 {
            return None;
        }
        if depth == self.uncertain.len() {
            if !changed {
                return None;
            }
            self.attempts += 1;
            return try_soft(symbols, self.fixed_erasures, self.confidences);
        }

        let index = self.uncertain[depth];
        let original = symbols[index];
        for candidate in &self.observations[index].candidates {
            if candidate.symbol == original {
                continue;
            }
            symbols[index] = candidate.symbol;
            if let Some(result) = self.search(symbols, depth + 1, true) {
                symbols[index] = original;
                return Some(result);
            }
        }
        symbols[index] = original;
        self.search(symbols, depth + 1, changed)
    }
}

pub fn decode_soft(observations: &[SoftObservation]) -> Result<Decoded, Error> {
    if observations.len() != SYMBOL_COUNT {
        return Err(Error::InvalidLength);
    }

    let mut symbols = Vec::with_capacity(SYMBOL_COUNT);
    let mut confidences = Vec::with_capacity(SYMBOL_COUNT);
    let mut fixed_erasures = Vec::new();
    let mut hypotheses = observations.to_vec();

    for (position, observation) in hypotheses.iter_mut().enumerate() {
        if observation.candidates.len() > 3 {
            return Err(Error::InvalidObservation);
        }
        observation
            .candidates
            .sort_by_key(|item| std::cmp::Reverse(item.confidence));
        for (index, item) in observation.candidates.iter().enumerate() {
            if item.symbol >= 64
                || observation.candidates[..index]
                    .iter()
                    .any(|other| other.symbol == item.symbol)
            {
                return Err(Error::InvalidObservation);
            }
        }

        if let Some(best) = observation.candidates.first() {
            symbols.push(best.symbol);
            confidences.push(best.confidence);
        } else {
            symbols.push(0);
            confidences.push(0);
            fixed_erasures.push(position);
        }
    }

    if fixed_erasures.len() > CORRECTION_SYMBOLS {
        return Err(Error::Uncorrectable);
    }
    if let Some(decoded) = try_soft(&symbols, &fixed_erasures, &confidences) {
        return Ok(decoded);
    }

    let mut uncertain: Vec<usize> = (0..SYMBOL_COUNT)
        .filter(|&position| hypotheses[position].candidates.len() > 1)
        .collect();
    uncertain.sort_by_key(|&position| {
        let choices = &hypotheses[position].candidates;
        (
            choices[0].confidence.saturating_sub(choices[1].confidence),
            position,
        )
    });
    uncertain.truncate(8);

    let mut search = SoftSearch {
        observations: &hypotheses,
        fixed_erasures: &fixed_erasures,
        confidences: &confidences,
        uncertain: &uncertain,
        attempts: 0,
    };
    search
        .search(&mut symbols, 0, false)
        .ok_or(Error::Uncorrectable)
}

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
    code.validate_moment()?;
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

    let code = MusicCode {
        track: identity::decode(rank)?,
        moment_seconds,
    };
    if code.validate_moment().is_err() {
        return Err(Error::InvalidCodeword);
    }
    Ok(Decoded {
        code,
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
            Track::YouTubeVideo("AAAAAAAAAAA".into()),
            Track::YouTubeVideo("___________".into()),
            Track::YouTubeShort("AAAAAAAAAAA".into()),
            Track::YouTubeShort("___________".into()),
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
            for moment_seconds in [
                None,
                Some(0),
                Some(187),
                Some(4095),
                Some(4096),
                Some(65535),
            ] {
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
    fn legacy_v1_symbols_are_unchanged_and_new_ranges_fit_the_same_frame() {
        let legacy = MusicCode {
            track: Track::YouTubeMusic("JGwWNGJdvx8".into()),
            moment_seconds: Some(187),
        };
        let original = encode(&legacy).unwrap();
        assert_eq!(decode(&original).unwrap().code, legacy);
        assert_eq!(original.len(), SYMBOL_COUNT);

        for track in [
            Track::YouTubeVideo("JGwWNGJdvx8".into()),
            Track::YouTubeShort("JGwWNGJdvx8".into()),
            Track::TikTokVideo(7_300_873_037_530_795_272),
            Track::InstagramReel("DIPPWkUh7Rj".into()),
        ] {
            let code = MusicCode {
                track,
                moment_seconds: None,
            };
            let symbols = encode(&code).unwrap();
            assert_eq!(symbols.len(), SYMBOL_COUNT);
            assert_eq!(decode(&symbols).unwrap().code, code);
        }
    }

    #[test]
    fn unsupported_native_moments_cannot_be_written() {
        for track in [
            Track::TikTokVideo(7_300_873_037_530_795_272),
            Track::InstagramReel("DIPPWkUh7Rj".into()),
        ] {
            let code = MusicCode {
                track,
                moment_seconds: Some(12),
            };
            assert_eq!(encode(&code), Err(Error::InvalidTimestamp));
        }
    }

    #[test]
    fn crc_rejects_a_valid_rs_word_with_modified_payload() {
        let code = MusicCode {
            track: Track::YouTubeMusic("JGwWNGJdvx8".into()),
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
