//! Reversible V2 source profiles. The W carries only the optical version.
//! Dual profiles 14 and 16 are inferred from exact optical column count;
//! the payload is CRC-16 checked and RS(4 parity bytes) protected.
use super::codec::{crc16, BitReader, BitWriter, Content, Decoded, Frame, Layout};
use crate::rank::Rank;
use crate::{reed_solomon, Error, MusicCode, Track};

const SPOTIFY: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const YOUTUBE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const PARITY: usize = reed_solomon::PARITY_SYMBOLS;

// There are 62^22 Base62 IDs. The 128-bit subset plus a canonical
// 64-bit YouTube ID already fills the 30-column optical class exactly.
// The 31-column class has 200 information bits. Its first contiguous
// range now covers EVERY no-time Spotify22 + YouTube66 pair, not just
// canonical YouTube IDs. Remaining ordinals carry short-time pairs.
// This is a bijective enumerative encoding, not a music-ID dictionary.
pub(super) const PACKED_DUAL_PROFILE: u8 = 16;
const SHORT_FULL_DUAL_PROFILE: u8 = 20;
const LONG_FULL_DUAL_PROFILE: u8 = 21;

// These profiles need no separate header column. The FIRST source bit is
// exactly one, leaving a disjoint marker from an explicit profile byte
// (all current explicit headers are <64). The marker is inside the
// systematic RS codeword and therefore receives full RS protection.
fn full_dual_frame(
    spotify: Rank,
    youtube: Rank,
    moment_seconds: Option<u16>,
) -> Result<Frame, Error> {
    let mut out = BitWriter::default();
    out.push(1, 1);
    match moment_seconds {
        None => {
            out.push(0, 1);
            out.rank(spotify, 131)?;
            out.rank(youtube, 66)?;
            out.push(0, 8);
            frame(SHORT_FULL_DUAL_PROFILE, out)
        }
        Some(moment) if moment <= 255 => {
            out.push(1, 1);
            out.rank(spotify, 131)?;
            out.rank(youtube, 66)?;
            out.push(u64::from(moment), 8);
            frame(SHORT_FULL_DUAL_PROFILE, out)
        }
        Some(moment) => {
            out.rank(spotify, 131)?;
            out.rank(youtube, 66)?;
            out.push(u64::from(moment), 16);
            frame(LONG_FULL_DUAL_PROFILE, out)
        }
    }
}

fn read_full_dual(header: u8, bits: &mut BitReader<'_>) -> Result<Content, Error> {
    if bits.take(1)? != 1 {
        return Err(Error::InvalidCodeword);
    }
    let has_short_time = if header == SHORT_FULL_DUAL_PROFILE {
        bits.take(1)? != 0
    } else {
        false
    };
    let spotify = bits
        .rank(131)?
        .to_digits(SPOTIFY, 22)
        .ok_or(Error::InvalidCodeword)?;
    let youtube = bits
        .rank(66)?
        .to_digits(YOUTUBE, 11)
        .ok_or(Error::InvalidCodeword)?;
    let moment = if header == SHORT_FULL_DUAL_PROFILE {
        let time = bits.take(8)? as u16;
        if !has_short_time && time != 0 {
            return Err(Error::InvalidCodeword);
        }
        has_short_time.then_some(time)
    } else {
        let time = bits.take(16)? as u16;
        if time <= 255 {
            return Err(Error::InvalidCodeword);
        }
        Some(time)
    };
    Ok(Content::Dual {
        spotify_id: spotify,
        youtube_music_id: youtube,
        moment_seconds: moment,
    })
}

/// Exact mixed-radix integer: Spotify * 2^66 + complete YouTube rank.
/// The first 17 bytes contain (Spotify << 2) plus the top two YouTube
/// bits; the last eight bytes contain the remaining 64 YouTube bits.
fn full_no_time_ordinal(spotify: Rank, youtube: Rank) -> Result<[u8; 25], Error> {
    if spotify > Rank::pow(62, 22) || youtube >= Rank::pow(2, 66) {
        return Err(Error::InvalidIdentifier);
    }
    let spotify_bytes = spotify.to_bytes(17).ok_or(Error::InvalidIdentifier)?;
    let youtube_bytes = youtube.to_bytes(9).ok_or(Error::InvalidIdentifier)?;
    let mut result = [0_u8; 25];
    let mut carry = 0_u16;
    for index in (0..17).rev() {
        let shifted = u16::from(spotify_bytes[index]) * 4 + carry;
        result[index] = shifted as u8;
        carry = shifted >> 8;
    }
    if carry != 0 || youtube_bytes[0] > 3 {
        return Err(Error::InvalidIdentifier);
    }
    result[16] |= youtube_bytes[0];
    result[17..].copy_from_slice(&youtube_bytes[1..]);
    Ok(result)
}

fn packed_boundary() -> Result<[u8; 25], Error> {
    full_no_time_ordinal(Rank::pow(62, 22), Rank::zero())
}

fn add_ordinal(left: [u8; 25], right: [u8; 25]) -> Option<[u8; 25]> {
    let mut result = [0_u8; 25];
    let mut carry = 0_u16;
    for i in (0..25).rev() {
        let sum = u16::from(left[i]) + u16::from(right[i]) + carry;
        result[i] = sum as u8;
        carry = sum >> 8;
    }
    (carry == 0).then_some(result)
}

fn subtract_ordinal(left: [u8; 25], right: [u8; 25]) -> Option<[u8; 25]> {
    if left < right {
        return None;
    }
    let mut result = [0_u8; 25];
    let mut borrow = 0_i16;
    for i in (0..25).rev() {
        let diff = i16::from(left[i]) - i16::from(right[i]) - borrow;
        result[i] = diff as u8;
        borrow = i16::from(diff < 0);
    }
    (borrow == 0).then_some(result)
}

fn packed_short_moment(spotify: Rank, youtube: u64, moment: u8) -> Option<[u8; 25]> {
    let prefix = spotify.to_bytes(16)?;
    let mut data = [0_u8; 25];
    data[..16].copy_from_slice(&prefix);
    data[16..24].copy_from_slice(&youtube.to_be_bytes());
    data[24] = moment;
    add_ordinal(packed_boundary().ok()?, data)
}

fn packed_dual_frame(data: [u8; 25]) -> Result<Frame, Error> {
    let mut writer = BitWriter::default();
    for byte in data {
        writer.push(u64::from(byte), 8);
    }
    frame(PACKED_DUAL_PROFILE, writer)
}

fn unpack_packed_dual(source: &[u8]) -> Result<Content, Error> {
    let source: [u8; 25] = source.try_into().map_err(|_| Error::InvalidLength)?;
    let boundary = packed_boundary()?;
    let (spotify, youtube_music_id, moment) = if source < boundary {
        let mut spotify_bytes = [0_u8; 17];
        let mut carry = 0_u8;
        for (index, byte) in source[..17].iter().copied().enumerate() {
            spotify_bytes[index] = (byte >> 2) | carry;
            carry = byte << 6;
        }
        let spotify = Rank::from_bytes(&spotify_bytes);
        if spotify >= Rank::pow(62, 22) {
            return Err(Error::InvalidCodeword);
        }
        let mut youtube_bytes = [0_u8; 9];
        youtube_bytes[0] = source[16] & 3;
        youtube_bytes[1..].copy_from_slice(&source[17..25]);
        let youtube = Rank::from_bytes(&youtube_bytes)
            .to_digits(YOUTUBE, 11)
            .ok_or(Error::InvalidCodeword)?;
        (spotify, youtube, None)
    } else {
        let delta = subtract_ordinal(source, boundary).ok_or(Error::InvalidCodeword)?;
        let spotify = Rank::from_bytes(&delta[..16]);
        let video = u64::from_be_bytes(delta[16..24].try_into().map_err(|_| Error::InvalidLength)?);
        (spotify, youtube_id(video)?, Some(u16::from(delta[24])))
    };
    Ok(Content::Dual {
        spotify_id: spotify
            .to_digits(SPOTIFY, 22)
            .ok_or(Error::InvalidCodeword)?,
        youtube_music_id,
        moment_seconds: moment,
    })
}

fn spotify_rank(id: &str) -> Option<Rank> {
    if id.len() != 22 {
        return None;
    }
    Rank::from_digits(id, SPOTIFY)
}

fn youtube_rank(id: &str) -> Option<u64> {
    if id.len() != 11 {
        return None;
    }
    let number = Rank::from_digits(id, YOUTUBE)?;
    let (quotient, remainder) = number.div_mod_small(4);
    (remainder == 0 && quotient < Rank::pow(2, 64)).then(|| quotient.low_u64())
}

fn youtube_id(rank: u64) -> Result<String, Error> {
    Rank::from_u64(rank)
        .mul_small(4)
        .to_digits(YOUTUBE, 11)
        .ok_or(Error::InvalidCodeword)
}

fn numeric_tag(track: &Track) -> Option<(u64, u64)> {
    Some(match track {
        Track::Deezer(value) => (0, *value),
        Track::Tidal(value) => (1, *value),
        Track::Qobuz(value) => (2, *value),
        Track::TikTokVideo(value) => (3, *value),
        _ => return None,
    })
}

fn numeric_track(tag: u64, value: u64) -> Result<Track, Error> {
    Ok(match tag {
        0 => Track::Deezer(value),
        1 => Track::Tidal(value),
        2 => Track::Qobuz(value),
        3 => Track::TikTokVideo(value),
        _ => return Err(Error::InvalidCodeword),
    })
}

fn frame(header: u8, payload: BitWriter) -> Result<Frame, Error> {
    let mut bytes = payload.finish();
    let mut protected = Vec::with_capacity(bytes.len() + 1);
    protected.push(header);
    protected.extend_from_slice(&bytes);
    bytes.extend_from_slice(&crc16(&protected).to_be_bytes());
    if bytes.len() + PARITY > 34 {
        return Err(Error::InvalidLength);
    }
    Ok(Frame {
        metadata: header,
        symbols: reed_solomon::encode(&bytes),
    })
}

// A length-specific Dual mode for an unusually small Spotify rank.
// The first bit marks a protected implicit header, the second bit
// distinguishes no timestamp from a complete 8-bit timestamp.
// For N columns the remaining capacity is 8N-48-2-64 = 8N-114
// Spotify bits (no time), or 8N-122 (with an 8-bit timestamp).
// N=15..26 consumes exactly N optical columns with CRC16+RS4.
fn small_dual_frame(
    spotify: Rank,
    youtube: u64,
    moment: Option<u16>,
) -> Result<Option<Frame>, Error> {
    // A protected 1-bit marker plus 63 bits of YouTube rank exactly
    // fills a 14-column word (64 source bits + CRC16 + RS32).
    if moment.is_none() && spotify == Rank::zero() && youtube < (1_u64 << 63) {
        let mut writer = BitWriter::default();
        writer.push(1, 1);
        writer.push(youtube, 63);
        return Ok(Some(frame(34, writer)?));
    }
    if moment.is_some_and(|value| value > 255) {
        return Ok(None);
    }
    for columns in 15_usize..=26 {
        if moment.is_some() && columns == 15 {
            continue;
        }
        let bits = 8 * columns - 114 - usize::from(moment.is_some()) * 8;
        if spotify >= Rank::pow(2, bits) {
            continue;
        }
        let mut writer = BitWriter::default();
        writer.push(1, 1);
        writer.push(u64::from(moment.is_some()), 1);
        writer.rank(spotify, bits)?;
        writer.push(youtube, 64);
        if let Some(seconds) = moment {
            writer.push(u64::from(seconds), 8);
        }
        return Ok(Some(frame((columns + 7) as u8, writer)?));
    }
    Ok(None)
}

pub(super) fn candidates(content: &Content) -> Result<Vec<Frame>, Error> {
    let mut result = Vec::new();
    match content {
        Content::Single(code) => {
            code.validate_moment()?;
            match &code.track {
                Track::YouTubeMusic(id) => {
                    if let Some(rank) = youtube_rank(id) {
                        match code.moment_seconds {
                            None => {
                                let mut out = BitWriter::default();
                                out.push(rank, 64);
                                result.push(frame(1, out)?);
                            }
                            Some(moment) if moment <= 255 => {
                                let mut out = BitWriter::default();
                                out.push(rank, 64);
                                out.push(u64::from(moment), 8);
                                result.push(frame(3, out)?);
                            }
                            Some(_) => {}
                        }
                    }
                }
                Track::Spotify(id) => {
                    if let Some(rank) = spotify_rank(id) {
                        let fits_128 = rank < Rank::pow(2, 128);
                        match code.moment_seconds {
                            None if fits_128 => {
                                let mut out = BitWriter::default();
                                out.rank(rank, 128)?;
                                result.push(frame(9, out)?);
                            }
                            Some(moment) if moment <= 255 => {
                                let mut out = BitWriter::default();
                                if fits_128 {
                                    out.rank(rank, 128)?;
                                    out.push(u64::from(moment), 8);
                                    result.push(frame(12, out)?);
                                }
                                let mut out = BitWriter::default();
                                out.rank(rank, 131)?;
                                out.push(u64::from(moment), 8);
                                result.push(frame(11, out)?);
                            }
                            _ => {}
                        }
                    }
                }
                track => {
                    if let Some((tag, value)) = numeric_tag(track) {
                        if let Ok(short_id) = u32::try_from(value) {
                            if code.moment_seconds.is_none() {
                                let mut out = BitWriter::default();
                                out.push(tag, 2);
                                out.push(u64::from(short_id), 32);
                                result.push(frame(5, out)?);
                            }
                            if let Some(moment) = code.moment_seconds {
                                if moment <= 255 && tag != 3 {
                                    let mut out = BitWriter::default();
                                    out.push(tag, 2);
                                    out.push(u64::from(short_id), 32);
                                    out.push(u64::from(moment), 8);
                                    result.push(frame(7, out)?);
                                }
                            }
                        }
                    }
                }
            }
        }
        Content::Dual {
            spotify_id,
            youtube_music_id,
            moment_seconds,
        } => {
            if let (Some(spotify), Some(youtube)) =
                (spotify_rank(spotify_id), youtube_rank(youtube_music_id))
            {
                if let Some(short) = small_dual_frame(spotify, youtube, *moment_seconds)? {
                    result.push(short);
                }
                let fits_128 = spotify < Rank::pow(2, 128);
                if moment_seconds.is_none() {
                    for (width, profile) in [(104_usize, 17_u8), (112, 18), (120, 19)] {
                        if spotify < Rank::pow(2, width) {
                            let mut out = BitWriter::default();
                            out.rank(spotify, width)?;
                            out.push(youtube, 64);
                            result.push(frame(profile, out)?);
                        }
                    }
                }
                match moment_seconds {
                    None if fits_128 => {
                        let mut out = BitWriter::default();
                        out.rank(spotify, 128)?;
                        out.push(youtube, 64);
                        result.push(frame(14, out)?);
                    }
                    None => {
                        // The full no-time mixed-radix class is added below,
                        // including every noncanonical 66-bit YouTube ID.
                    }
                    Some(moment) if *moment <= 255 => {
                        if fits_128 {
                            if let Some(value) =
                                packed_short_moment(spotify, youtube, *moment as u8)
                            {
                                result.push(packed_dual_frame(value)?);
                            }
                            // A complete fallback for short-time pairs not
                            // contained in the remaining 31-column ordinal
                            // interval after every no-time pair is reserved.
                            let mut out = BitWriter::default();
                            out.rank(spotify, 128)?;
                            out.push(youtube, 64);
                            out.push(u64::from(*moment), 8);
                            result.push(frame(15, out)?);
                        }
                        let mut out = BitWriter::default();
                        out.rank(spotify, 131)?;
                        out.push(youtube, 64);
                        out.push(u64::from(*moment), 8);
                        result.push(frame(13, out)?);
                    }
                    _ => {}
                }
            }
        }
    }
    if let Content::Dual {
        spotify_id,
        youtube_music_id,
        moment_seconds,
    } = content
    {
        if let Some(spotify) = spotify_rank(spotify_id) {
            if youtube_music_id.len() == 11 {
                if let Some(youtube) = Rank::from_digits(youtube_music_id, YOUTUBE) {
                    if moment_seconds.is_none() {
                        result.push(packed_dual_frame(full_no_time_ordinal(spotify, youtube)?)?);
                    }
                    result.push(full_dual_frame(spotify, youtube, *moment_seconds)?);
                }
            }
        }
    }
    Ok(result)
}

pub(super) fn decode(code: &Frame) -> Result<Decoded, Error> {
    let (layout, rank_width, short_moment) = match code.metadata {
        1 => (Layout::Compact, 64, false),
        3 => (Layout::Compact, 64, true),
        5 => (Layout::Compact, 32, false),
        7 => (Layout::Compact, 32, true),
        9 => (Layout::Full, 128, false),
        11 => (Layout::Full, 131, true),
        12 => (Layout::Full, 128, true),
        13 => (Layout::Dual, 131, true),
        14 => (Layout::Dual, 128, false),
        15 => (Layout::Dual, 128, true),
        PACKED_DUAL_PROFILE => (Layout::Dual, 0, false),
        17 => (Layout::Dual, 104, false),
        18 => (Layout::Dual, 112, false),
        19 => (Layout::Dual, 120, false),
        SHORT_FULL_DUAL_PROFILE | LONG_FULL_DUAL_PROFILE => (Layout::Dual, 0, false),
        22..=33 => (Layout::Dual, 0, false),
        34 => (Layout::Dual, 0, false),
        _ => return Err(Error::UnsupportedVersion),
    };
    if code.metadata == PACKED_DUAL_PROFILE && code.symbols.len() != 31 {
        return Err(Error::InvalidLength);
    }
    if (22..=33).contains(&code.metadata)
        && code.symbols.len() != usize::from(code.metadata) - 7
    {
        return Err(Error::InvalidLength);
    }
    if code.metadata == 34 && code.symbols.len() != 14 {
        return Err(Error::InvalidLength);
    }
    if matches!(
        code.metadata,
        SHORT_FULL_DUAL_PROFILE | LONG_FULL_DUAL_PROFILE
    ) && !matches!(
        (code.metadata, code.symbols.len()),
        (SHORT_FULL_DUAL_PROFILE, 32) | (LONG_FULL_DUAL_PROFILE, 33)
    ) {
        return Err(Error::InvalidLength);
    }
    let (corrected, corrected_symbols) = reed_solomon::decode(&code.symbols)?;
    if corrected.len() < PARITY + 3 || corrected.len() > 34 {
        return Err(Error::InvalidLength);
    }
    let message = &corrected[..corrected.len() - PARITY];
    let payload = &message[..message.len() - 2];
    let crc = u16::from_be_bytes([message[message.len() - 2], message[message.len() - 1]]);
    let mut protected = Vec::with_capacity(payload.len() + 1);
    protected.push(code.metadata);
    protected.extend_from_slice(payload);
    if crc16(&protected) != crc {
        return Err(Error::ChecksumMismatch);
    }
    let mut bits = BitReader::new(payload);
    let content = match code.metadata {
        1 | 3 => {
            let id = youtube_id(bits.take(64)?)?;
            let moment = if short_moment {
                Some(bits.take(8)? as u16)
            } else {
                None
            };
            Content::Single(MusicCode {
                track: Track::YouTubeMusic(id),
                moment_seconds: moment,
            })
        }
        5 | 7 => {
            let tag = bits.take(2)?;
            if code.metadata == 7 && tag == 3 {
                return Err(Error::InvalidCodeword);
            }
            let id = bits.take(32)?;
            let moment = if short_moment {
                Some(bits.take(8)? as u16)
            } else {
                None
            };
            Content::Single(MusicCode {
                track: numeric_track(tag, id)?,
                moment_seconds: moment,
            })
        }
        9 | 11 | 12 => {
            let id = bits
                .rank(rank_width)?
                .to_digits(SPOTIFY, 22)
                .ok_or(Error::InvalidCodeword)?;
            let moment = if short_moment {
                Some(bits.take(8)? as u16)
            } else {
                None
            };
            Content::Single(MusicCode {
                track: Track::Spotify(id),
                moment_seconds: moment,
            })
        }
        PACKED_DUAL_PROFILE => unpack_packed_dual(payload)?,
        34 => {
            if bits.take(1)? != 1 {
                return Err(Error::InvalidCodeword);
            }
            Content::Dual {
                spotify_id: "0000000000000000000000".into(),
                youtube_music_id: youtube_id(bits.take(63)?)?,
                moment_seconds: None,
            }
        }
        22..=33 => {
            if bits.take(1)? != 1 {
                return Err(Error::InvalidCodeword);
            }
            let with_moment = bits.take(1)? != 0;
            let columns = usize::from(code.metadata) - 7;
            if with_moment && columns == 15 {
                return Err(Error::InvalidCodeword);
            }
            let width = 8 * columns - 114 - usize::from(with_moment) * 8;
            let spotify_id = bits
                .rank(width)?
                .to_digits(SPOTIFY, 22)
                .ok_or(Error::InvalidCodeword)?;
            let youtube_music_id = youtube_id(bits.take(64)?)?;
            let moment_seconds = if with_moment {
                Some(bits.take(8)? as u16)
            } else {
                None
            };
            Content::Dual {
                spotify_id,
                youtube_music_id,
                moment_seconds,
            }
        },
        SHORT_FULL_DUAL_PROFILE | LONG_FULL_DUAL_PROFILE => {
            read_full_dual(code.metadata, &mut bits)?
        }
        13..=15 | 17..=19 => {
            let spotify_id = bits
                .rank(rank_width)?
                .to_digits(SPOTIFY, 22)
                .ok_or(Error::InvalidCodeword)?;
            let youtube_music_id = youtube_id(bits.take(64)?)?;
            let moment = if short_moment {
                Some(bits.take(8)? as u16)
            } else {
                None
            };
            Content::Dual {
                spotify_id,
                youtube_music_id,
                moment_seconds: moment,
            }
        }
        _ => return Err(Error::UnsupportedVersion),
    };
    if code.metadata != PACKED_DUAL_PROFILE {
        bits.finish()?;
    }
    if let Content::Single(code) = &content {
        code.validate_moment().map_err(|_| Error::InvalidCodeword)?;
    }
    Ok(Decoded {
        content,
        corrected_symbols,
        layout,
    })
}
