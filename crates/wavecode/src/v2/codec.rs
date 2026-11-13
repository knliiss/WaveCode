//! Experimental short single-link and two-link payloads.
//! Each GF(256) byte is represented by two 16-level optical rails;
//! IDs are encoded losslessly without an identity database.
use crate::identity;
use crate::rank::Rank;
use crate::{reed_solomon, Error, MusicCode, Track};

const YOUTUBE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const SPOTIFY: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const AMAZON: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const PARITY_BYTES: usize = reed_solomon::PARITY_SYMBOLS;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Layout {
    Compact,
    Full,
    Dual,
}

impl Layout {
    fn bits(self) -> u8 {
        match self {
            Self::Compact => 0,
            Self::Full => 1,
            Self::Dual => 2,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Content {
    Single(MusicCode),
    Dual {
        spotify_id: String,
        youtube_music_id: String,
        moment_seconds: Option<u16>,
    },
}

impl Content {
    fn moment(&self) -> Option<u16> {
        match self {
            Self::Single(code) => code.moment_seconds,
            Self::Dual { moment_seconds, .. } => *moment_seconds,
        }
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Frame {
    pub metadata: u8,
    /// Systematic RS codeword; each byte is one optical column.
    pub symbols: Vec<u8>,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decoded {
    pub content: Content,
    pub corrected_symbols: usize,
    pub layout: Layout,
}

#[derive(Default)]
pub(super) struct BitWriter {
    bits: Vec<bool>,
}

impl BitWriter {
    pub(super) fn push(&mut self, value: u64, width: usize) {
        for shift in (0..width).rev() {
            self.bits.push((value >> shift) & 1 != 0);
        }
    }

    pub(super) fn rank(&mut self, rank: Rank, width: usize) -> Result<(), Error> {
        let bytes = rank
            .to_bytes(width.div_ceil(8))
            .ok_or(Error::InvalidIdentifier)?;
        let offset = (8 - width % 8) % 8;
        for index in 0..width {
            let position = offset + index;
            self.bits
                .push(bytes[position / 8] & (1 << (7 - position % 8)) != 0);
        }
        Ok(())
    }

    pub(super) fn finish(mut self) -> Vec<u8> {
        while self.bits.len() % 8 != 0 {
            self.bits.push(false);
        }
        self.bits
            .chunks(8)
            .map(|chunk| {
                chunk
                    .iter()
                    .fold(0_u8, |sum, &bit| (sum << 1) | u8::from(bit))
            })
            .collect()
    }
}

pub(super) struct BitReader<'a> {
    bytes: &'a [u8],
    cursor: usize,
}

impl<'a> BitReader<'a> {
    pub(super) fn new(bytes: &'a [u8]) -> Self {
        Self { bytes, cursor: 0 }
    }

    pub(super) fn take(&mut self, width: usize) -> Result<u64, Error> {
        if width > 64 || self.cursor + width > self.bytes.len() * 8 {
            return Err(Error::InvalidCodeword);
        }
        let mut value = 0_u64;
        for _ in 0..width {
            let position = self.cursor;
            value = (value << 1) | u64::from((self.bytes[position / 8] >> (7 - position % 8)) & 1);
            self.cursor += 1;
        }
        Ok(value)
    }

    pub(super) fn rank(&mut self, width: usize) -> Result<Rank, Error> {
        let mut result = Rank::zero();
        for _ in 0..width {
            result = result.mul_small(2).add(Rank::from_u64(self.take(1)?));
        }
        Ok(result)
    }

    pub(super) fn finish(&self) -> Result<(), Error> {
        if self.bytes.len() * 8 - self.cursor >= 8 {
            return Err(Error::InvalidCodeword);
        }
        let mut check = Self {
            bytes: self.bytes,
            cursor: self.cursor,
        };
        while check.cursor < check.bytes.len() * 8 {
            if check.take(1)? != 0 {
                return Err(Error::InvalidCodeword);
            }
        }
        Ok(())
    }
}

pub(super) fn crc16(bytes: &[u8]) -> u16 {
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

fn compact_track(code: &Track, out: &mut BitWriter) -> Result<(), Error> {
    match code {
        Track::YouTubeMusic(id) | Track::YouTubeVideo(id) | Track::YouTubeShort(id) => {
            let tag = match code {
                Track::YouTubeMusic(_) => 0,
                Track::YouTubeVideo(_) => 1,
                _ => 2,
            };
            if id.len() != 11 {
                return Err(Error::InvalidIdentifier);
            }
            out.push(tag, 4);
            out.rank(
                Rank::from_digits(id, YOUTUBE).ok_or(Error::InvalidIdentifier)?,
                66,
            )?;
        }
        Track::TikTokVideo(value)
        | Track::Deezer(value)
        | Track::Tidal(value)
        | Track::Qobuz(value) => {
            let tag = match code {
                Track::TikTokVideo(_) => 3,
                Track::Deezer(_) => 5,
                Track::Tidal(_) => 6,
                _ => 7,
            };
            out.push(tag, 4);
            out.push(*value, 64);
        }
        Track::AppleMusic { id, storefront } => {
            let text = storefront.as_bytes();
            if text.len() != 2 || !text.iter().all(u8::is_ascii_lowercase) {
                return Err(Error::InvalidStorefront);
            }
            let region = u64::from(text[0] - b'a') * 26 + u64::from(text[1] - b'a');
            out.push(4, 4);
            out.push(region, 10);
            out.push(*id, 64);
        }
        Track::AmazonMusic(asin) => {
            if asin.len() != 10 {
                return Err(Error::InvalidIdentifier);
            }
            out.push(8, 4);
            out.rank(
                Rank::from_digits(asin, AMAZON).ok_or(Error::InvalidIdentifier)?,
                52,
            )?;
        }
        Track::InstagramReel(slug) => {
            if slug.is_empty() || slug.len() > 16 {
                return Err(Error::InvalidIdentifier);
            }
            out.push(9, 4);
            out.push((slug.len() - 1) as u64, 4);
            out.rank(
                Rank::from_digits(slug, YOUTUBE).ok_or(Error::InvalidIdentifier)?,
                slug.len() * 6,
            )?;
        }
        Track::Spotify(_) => return Err(Error::InvalidIdentifier),
    }
    Ok(())
}

fn read_compact(reader: &mut BitReader<'_>) -> Result<Track, Error> {
    let tag = reader.take(4)?;
    Ok(match tag {
        0..=2 => {
            let id = reader
                .rank(66)?
                .to_digits(YOUTUBE, 11)
                .ok_or(Error::InvalidCodeword)?;
            match tag {
                0 => Track::YouTubeMusic(id),
                1 => Track::YouTubeVideo(id),
                _ => Track::YouTubeShort(id),
            }
        }
        3 => Track::TikTokVideo(reader.take(64)?),
        4 => {
            let region = reader.take(10)?;
            if region >= 676 {
                return Err(Error::InvalidCodeword);
            }
            Track::AppleMusic {
                id: reader.take(64)?,
                storefront: format!(
                    "{}{}",
                    (b'a' + (region / 26) as u8) as char,
                    (b'a' + (region % 26) as u8) as char
                ),
            }
        }
        5 => Track::Deezer(reader.take(64)?),
        6 => Track::Tidal(reader.take(64)?),
        7 => Track::Qobuz(reader.take(64)?),
        8 => {
            let asin = reader
                .rank(52)?
                .to_digits(AMAZON, 10)
                .ok_or(Error::InvalidCodeword)?;
            Track::AmazonMusic(asin)
        }
        9 => {
            let len = reader.take(4)? as usize + 1;
            let slug = reader
                .rank(len * 6)?
                .to_digits(YOUTUBE, len)
                .ok_or(Error::InvalidCodeword)?;
            Track::InstagramReel(slug)
        }
        _ => return Err(Error::InvalidCodeword),
    })
}

fn make_header(layout: Layout, has_time: bool) -> u8 {
    layout.bits() << 2 | (u8::from(has_time) << 1)
}

fn parse_header(meta: u8) -> Result<(Layout, bool), Error> {
    if meta & 1 != 0 {
        return Err(Error::InvalidCodeword);
    }
    let layout = match meta >> 2 {
        0 => Layout::Compact,
        1 => Layout::Full,
        2 => Layout::Dual,
        _ => return Err(Error::UnsupportedVersion),
    };
    Ok((layout, meta & 2 != 0))
}

pub fn encode(content: &Content) -> Result<Frame, Error> {
    let layout = match content {
        Content::Single(code) if matches!(code.track, Track::Spotify(_)) => Layout::Full,
        Content::Single(_) => Layout::Compact,
        Content::Dual { .. } => Layout::Dual,
    };
    encode_with_layout(content, layout)
}

/// Choose the shortest supported experimental frame, preserving a legacy V2
/// fallback for every identifier and moment. This is opt-in: `encode` is unchanged.
/// Metadata is normally one extra optical byte. Two V2 Dual classes are
/// unambiguously self-delimiting by their Reed-Solomon codeword length:
/// 27-29 columns -> canonical dual with 104, 112 or 120-bit Spotify
/// rank. 30 columns -> 128-bit Spotify rank without timestamp.
/// 31 columns -> mixed-radix no-time/short-time dual (profile 16).
/// All are derived from the number of actual visible optical columns,
/// independent of rendering gaps and style.
pub fn implicit_dual_profile(optical_columns: usize) -> Option<u8> {
    match optical_columns {
        27 => Some(17),
        28 => Some(18),
        29 => Some(19),
        30 => Some(14),
        31 => Some(16),
        _ => None,
    }
}

/// Profile 34: 14 visible Dual columns when Spotify rank=0 and the
/// canonical YouTube rank fits 63 bits. Profile 22..33: 15..26
/// visible Dual columns for unusually small
/// Spotify ranks with canonical YouTube IDs. Their first protected source
/// bit is one, disjoint from any existing explicit metadata byte (<64).
/// The next bit distinguishes no moment (0) and an 8-bit moment (1).
/// The CRC includes the implicit profile, and RS protects the marker.
pub fn marked_dual_profile(columns: usize) -> Option<u8> {
    match columns {
        14 => Some(34),
        15..=26 => Some((columns + 7) as u8),
        32 => Some(20),
        33 => Some(21),
        _ => None,
    }
}

/// The metadata header is omitted only when the protected marker or exact
/// column count determines the mode; styling, W spacing and colors don't.
pub fn implicit_dual_profile_for_frame(frame: &Frame) -> bool {
    implicit_dual_profile(frame.symbols.len()) == Some(frame.metadata)
        || marked_dual_profile(frame.symbols.len()) == Some(frame.metadata)
}

pub fn optical_column_count(frame: &Frame) -> usize {
    frame.symbols.len() + usize::from(!implicit_dual_profile_for_frame(frame))
}

pub fn encode_adaptive(content: &Content) -> Result<Frame, Error> {
    let mut shortest = encode(content)?;
    for candidate in super::adaptive::candidates(content)? {
        let shorter = optical_column_count(&candidate) < optical_column_count(&shortest);
        let stronger_equal_length = optical_column_count(&candidate)
            == optical_column_count(&shortest)
            && implicit_dual_profile_for_frame(&candidate)
            && !implicit_dual_profile_for_frame(&shortest);
        if shorter || stronger_equal_length {
            shortest = candidate;
        }
    }
    Ok(shortest)
}

pub fn encode_with_layout(content: &Content, layout: Layout) -> Result<Frame, Error> {
    let has_time = content.moment().is_some();
    let metadata = make_header(layout, has_time);
    let mut out = BitWriter::default();
    match (content, layout) {
        (Content::Single(code), Layout::Compact) => {
            code.validate_moment()?;
            compact_track(&code.track, &mut out)?;
        }
        (Content::Single(code), Layout::Full) => {
            code.validate_moment()?;
            out.rank(identity::encode(&code.track)?, 131)?;
        }
        (
            Content::Dual {
                spotify_id,
                youtube_music_id,
                ..
            },
            Layout::Dual,
        ) => {
            if spotify_id.len() != 22 || youtube_music_id.len() != 11 {
                return Err(Error::InvalidIdentifier);
            }
            out.rank(
                Rank::from_digits(spotify_id, SPOTIFY).ok_or(Error::InvalidIdentifier)?,
                131,
            )?;
            out.rank(
                Rank::from_digits(youtube_music_id, YOUTUBE).ok_or(Error::InvalidIdentifier)?,
                66,
            )?;
        }
        _ => return Err(Error::InvalidCodeword),
    }
    if let Some(moment) = content.moment() {
        out.push(u64::from(moment), 16);
    }
    let mut bytes = out.finish();
    let mut checked = vec![metadata];
    checked.extend_from_slice(&bytes);
    bytes.extend_from_slice(&crc16(&checked).to_be_bytes());
    if bytes.len() + PARITY_BYTES > 34 {
        return Err(Error::InvalidLength);
    }
    // The optical W holds ONLY the version. Routing metadata is transmitted
    // as the first byte of the payload's optical column sequence and is
    // covered by CRC together with the content.
    Ok(Frame {
        metadata,
        symbols: reed_solomon::encode(&bytes),
    })
}

pub fn decode(frame: &Frame) -> Result<Decoded, Error> {
    if !matches!(frame.metadata, 0 | 2 | 4 | 6 | 8 | 10) {
        return super::adaptive::decode(frame);
    }
    let (layout, has_time) = parse_header(frame.metadata)?;
    let (recovered, corrected_symbols) = reed_solomon::decode(&frame.symbols)?;
    if recovered.len() < PARITY_BYTES + 3 || recovered.len() > 34 {
        return Err(Error::InvalidLength);
    }
    let data = &recovered[..recovered.len() - PARITY_BYTES];
    let payload = &data[..data.len() - 2];
    let actual = u16::from_be_bytes([data[data.len() - 2], data[data.len() - 1]]);
    let mut checked = vec![frame.metadata];
    checked.extend_from_slice(payload);
    if actual != crc16(&checked) {
        return Err(Error::ChecksumMismatch);
    }
    let mut reader = BitReader::new(payload);
    let content = match layout {
        Layout::Compact => {
            let track = read_compact(&mut reader)?;
            let moment_seconds = if has_time {
                Some(reader.take(16)? as u16)
            } else {
                None
            };
            let code = MusicCode {
                track,
                moment_seconds,
            };
            if code.validate_moment().is_err() {
                return Err(Error::InvalidCodeword);
            }
            Content::Single(code)
        }
        Layout::Full => {
            let track = identity::decode(reader.rank(131)?)?;
            let moment_seconds = if has_time {
                Some(reader.take(16)? as u16)
            } else {
                None
            };
            let code = MusicCode {
                track,
                moment_seconds,
            };
            if code.validate_moment().is_err() {
                return Err(Error::InvalidCodeword);
            }
            Content::Single(code)
        }
        Layout::Dual => {
            let spotify_id = reader
                .rank(131)?
                .to_digits(SPOTIFY, 22)
                .ok_or(Error::InvalidCodeword)?;
            let youtube_music_id = reader
                .rank(66)?
                .to_digits(YOUTUBE, 11)
                .ok_or(Error::InvalidCodeword)?;
            let moment_seconds = if has_time {
                Some(reader.take(16)? as u16)
            } else {
                None
            };
            Content::Dual {
                spotify_id,
                youtube_music_id,
                moment_seconds,
            }
        }
    };
    reader.finish()?;
    Ok(Decoded {
        content,
        corrected_symbols,
        layout,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_single_platform_roundtrips_with_shorter_payload() {
        for link in [
            "https://music.youtube.com/watch?v=JGwWNGJdvx8&t=80",
            "https://www.youtube.com/shorts/JGwWNGJdvx8",
            "https://open.spotify.com/track/0VjIjW4GlUZAMYd2vXMi3b",
            "https://music.apple.com/ua/song/12345",
            "https://www.deezer.com/track/18446744073709551615",
            "https://tidal.com/browse/track/12345",
            "https://open.qobuz.com/track/12345",
            "https://music.amazon.com/tracks/B012345678",
            "https://www.instagram.com/reel/DIPPWkUh7Rj/",
            "https://www.tiktok.com/@creator/video/7300873037530795272",
        ] {
            let input = Content::Single(MusicCode::from_url(link).unwrap());
            let encoded = encode(&input).unwrap();
            let decoded = decode(&encoded).unwrap();
            assert_eq!(decoded.content, input, "{link}");
            assert!(encoded.symbols.len() < 34, "{link}");
        }
    }

    #[test]
    fn dual_full_space_and_timestamp_roundtrip() {
        for (spotify_id, youtube_music_id) in [
            ("0000000000000000000000", "AAAAAAAAAAA"),
            ("zzzzzzzzzzzzzzzzzzzzzz", "___________"),
            ("0VjIjW4GlUZAMYd2vXMi3b", "JGwWNGJdvx8"),
        ] {
            for moment_seconds in [None, Some(0), Some(65535)] {
                let input = Content::Dual {
                    spotify_id: spotify_id.into(),
                    youtube_music_id: youtube_music_id.into(),
                    moment_seconds,
                };
                let encoded = encode(&input).unwrap();
                assert!(encoded.symbols.len() <= 34);
                let restored = decode(&encoded).unwrap();
                assert_eq!(restored.content, input);
                assert_eq!(restored.layout, Layout::Dual);
            }
        }
    }

    #[test]
    fn rs_repairs_two_byte_errors() {
        let input = Content::Dual {
            spotify_id: "zzzzzzzzzzzzzzzzzzzzzz".into(),
            youtube_music_id: "___________".into(),
            moment_seconds: Some(137),
        };
        let mut frame = encode(&input).unwrap();
        frame.symbols[1] ^= 0x5b;
        frame.symbols[10] ^= 0xc6;
        let recovered = decode(&frame).unwrap();
        assert_eq!(recovered.content, input);
        assert_eq!(recovered.corrected_symbols, 2);
    }

    #[test]
    fn metadata_mismatch_fails_crc() {
        let code = Content::Single(
            MusicCode::from_url("https://www.youtube.com/watch?v=JGwWNGJdvx8").unwrap(),
        );
        let mut encoded = encode(&code).unwrap();
        encoded.metadata |= 2;
        assert!(decode(&encoded).is_err());
    }
}
