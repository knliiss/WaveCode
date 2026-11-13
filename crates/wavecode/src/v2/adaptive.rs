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
// The 31-column class has 200 information bits (27 data bytes including
// CRC16, and four RS parity bytes). Fill its first range with every
// high-Spotify/no-time pair; use the remaining range for short-time pairs.
// This is a bijective enumerative encoding, not a music-ID dictionary.
pub(super) const PACKED_DUAL_PROFILE: u8 = 16;

fn packed_boundary() -> Result<[u8; 25], Error> {
    let high_spotify_count = Rank::pow(62, 22).sub(Rank::pow(2, 128));
    let prefix = high_spotify_count
        .to_bytes(17)
        .ok_or(Error::InvalidCodeword)?;
    let mut result = [0_u8; 25];
    result[..17].copy_from_slice(&prefix);
    Ok(result)
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

fn packed_high_no_time(spotify: Rank, youtube: u64) -> Result<[u8; 25], Error> {
    let minimum = Rank::pow(2, 128);
    if spotify < minimum || spotify >= Rank::pow(62, 22) {
        return Err(Error::InvalidIdentifier);
    }
    let offset = spotify
        .sub(minimum)
        .to_bytes(17)
        .ok_or(Error::InvalidIdentifier)?;
    let mut output = [0_u8; 25];
    output[..17].copy_from_slice(&offset);
    output[17..].copy_from_slice(&youtube.to_be_bytes());
    Ok(output)
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
    let (spotify, youtube, moment) = if source < boundary {
        let rank = Rank::from_bytes(&source[..17]).add(Rank::pow(2, 128));
        if rank >= Rank::pow(62, 22) {
            return Err(Error::InvalidCodeword);
        }
        let video = u64::from_be_bytes(
            source[17..25]
                .try_into()
                .map_err(|_| Error::InvalidLength)?,
        );
        (rank, video, None)
    } else {
        let delta = subtract_ordinal(source, boundary).ok_or(Error::InvalidCodeword)?;
        let rank = Rank::from_bytes(&delta[..16]);
        let video = u64::from_be_bytes(delta[16..24].try_into().map_err(|_| Error::InvalidLength)?);
        (rank, video, Some(u16::from(delta[24])))
    };
    Ok(Content::Dual {
        spotify_id: spotify
            .to_digits(SPOTIFY, 22)
            .ok_or(Error::InvalidCodeword)?,
        youtube_music_id: youtube_id(youtube)?,
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
                let fits_128 = spotify < Rank::pow(2, 128);
                match moment_seconds {
                    None if fits_128 => {
                        let mut out = BitWriter::default();
                        out.rank(spotify, 128)?;
                        out.push(youtube, 64);
                        result.push(frame(14, out)?);
                    }
                    None => {
                        // Every 22-character Base62 ID above 2^128 is
                        // represented exactly in the 31-column class.
                        result.push(packed_dual_frame(packed_high_no_time(spotify, youtube)?)?);
                    }
                    Some(moment) if *moment <= 255 => {
                        if fits_128 {
                            if let Some(value) =
                                packed_short_moment(spotify, youtube, *moment as u8)
                            {
                                result.push(packed_dual_frame(value)?);
                            }
                            // A valid 32-column fallback for the ~2.72%
                            // of the 128-bit domain outside the 31-column range.
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
        _ => return Err(Error::UnsupportedVersion),
    };
    if code.metadata == PACKED_DUAL_PROFILE && code.symbols.len() != 31 {
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
        13..=15 => {
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
