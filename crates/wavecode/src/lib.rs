mod identity;
mod rank;
mod reed_solomon;
mod scrambling;
pub mod v1;
mod visual;

use rank::Rank;
use reed_solomon::PARITY_SYMBOLS;

pub use visual::{from_symbols as bars_from_symbols, to_symbols as symbols_from_bars, Bar};

pub const TRACK_SYMBOLS: usize = 21;
pub const SHORT_MOMENT_SYMBOLS: usize = 22;
pub const LONG_MOMENT_SYMBOLS: usize = 23;
pub const MAX_MOMENT_SECONDS: u16 = u16::MAX;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Orientation {
    Upright,
    UpsideDown,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct OrientedDecoded {
    pub decoded: Decoded,
    pub orientation: Orientation,
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub enum Track {
    YouTubeMusic(String),
    YouTubeVideo(String),
    YouTubeShort(String),
    TikTokVideo(u64),
    TikTokShort(String),
    InstagramReel(String),
    Spotify(String),
    AppleMusic { id: u64, storefront: String },
    Deezer(u64),
    Tidal(u64),
    Qobuz(u64),
    AmazonMusic(String),
}

impl Track {
    pub fn from_url(url: &str) -> Result<Self, Error> {
        let rest = url.strip_prefix("https://").ok_or(Error::UnsupportedUrl)?;
        let (host, rest) = rest.split_once('/').ok_or(Error::UnsupportedUrl)?;
        if host.contains('@') || host.contains(':') {
            return Err(Error::UnsupportedUrl);
        }
        let fragment_free = rest.split('#').next().unwrap_or(rest);
        let (path, query) = fragment_free.split_once('?').unwrap_or((fragment_free, ""));
        let segments: Vec<&str> = path.split('/').filter(|part| !part.is_empty()).collect();

        match host {
            "music.youtube.com" if segments.as_slice() == ["watch"] => {
                let id = parameter(query, "v").ok_or(Error::InvalidIdentifier)?;
                let track = Self::YouTubeMusic(id.to_owned());
                identity::encode(&track)?;
                Ok(track)
            }
            "www.youtube.com" | "youtube.com" | "m.youtube.com"
                if segments.as_slice() == ["watch"] =>
            {
                let id = parameter(query, "v").ok_or(Error::InvalidIdentifier)?;
                let track = Self::YouTubeVideo(id.to_owned());
                identity::encode(&track)?;
                Ok(track)
            }
            "www.youtube.com" | "youtube.com" | "m.youtube.com"
                if segments.len() == 2 && segments[0] == "shorts" =>
            {
                let track = Self::YouTubeShort(segments[1].to_owned());
                identity::encode(&track)?;
                Ok(track)
            }
            "youtu.be" if segments.len() == 1 => {
                let track = Self::YouTubeVideo(segments[0].to_owned());
                identity::encode(&track)?;
                Ok(track)
            }
            "www.tiktok.com" | "tiktok.com" if segments.len() == 3 && segments[1] == "video" => {
                let handle = segments[0];
                if !handle.starts_with('@')
                    || !handle[1..]
                        .bytes()
                        .all(|c| c.is_ascii_alphanumeric() || c == b'_' || c == b'.')
                    || handle.len() > 25
                {
                    return Err(Error::InvalidIdentifier);
                }
                let id = segments[2]
                    .parse::<u64>()
                    .map_err(|_| Error::InvalidIdentifier)?;
                if id == 0 {
                    return Err(Error::InvalidIdentifier);
                }
                Ok(Self::TikTokVideo(id))
            }
            "vt.tiktok.com" if segments.len() == 1 => {
                let track = Self::TikTokShort(segments[0].to_owned());
                identity::encode(&track)?;
                Ok(track)
            }
            "www.instagram.com" | "instagram.com"
                if segments.len() == 2 && (segments[0] == "reel" || segments[0] == "reels") =>
            {
                let track = Self::InstagramReel(segments[1].to_owned());
                identity::encode(&track)?;
                Ok(track)
            }
            "open.spotify.com" if segments.len() == 2 && segments[0] == "track" => {
                let track = Self::Spotify(segments[1].to_owned());
                identity::encode(&track)?;
                Ok(track)
            }
            "www.deezer.com" | "deezer.com" if segments.len() == 2 && segments[0] == "track" => {
                let id = segments[1]
                    .parse::<u64>()
                    .map_err(|_| Error::InvalidIdentifier)?;
                Ok(Self::Deezer(id))
            }
            "tidal.com" | "www.tidal.com" | "listen.tidal.com"
                if (segments.len() == 2 && segments[0] == "track")
                    || (segments.len() == 3
                        && segments[0] == "browse"
                        && segments[1] == "track") =>
            {
                let id = segments
                    .last()
                    .ok_or(Error::InvalidIdentifier)?
                    .parse::<u64>()
                    .map_err(|_| Error::InvalidIdentifier)?;
                Ok(Self::Tidal(id))
            }
            "open.qobuz.com" | "play.qobuz.com"
                if segments.len() == 2 && segments[0] == "track" =>
            {
                let id = segments[1]
                    .parse::<u64>()
                    .map_err(|_| Error::InvalidIdentifier)?;
                Ok(Self::Qobuz(id))
            }
            "music.amazon.com"
                if (segments.len() == 2 && segments[0] == "tracks")
                    || (segments.len() == 2
                        && segments[0] == "albums"
                        && parameter(query, "trackAsin").is_some()) =>
            {
                let asin = if segments[0] == "tracks" {
                    segments[1]
                } else {
                    parameter(query, "trackAsin").ok_or(Error::InvalidIdentifier)?
                };
                let track = Self::AmazonMusic(asin.to_owned());
                identity::encode(&track)?;
                Ok(track)
            }
            "music.apple.com" if segments.len() >= 3 => {
                let storefront = segments[0].to_owned();
                let song_id = match segments[1] {
                    "song" => segments.last().and_then(|id| id.parse::<u64>().ok()),
                    "album" => parameter(query, "i").and_then(|id| id.parse::<u64>().ok()),
                    _ => None,
                };
                let track = Self::AppleMusic {
                    id: song_id.ok_or(Error::InvalidIdentifier)?,
                    storefront,
                };
                identity::encode(&track)?;
                Ok(track)
            }
            _ => Err(Error::UnsupportedUrl),
        }
    }

    pub fn to_url(&self) -> Result<String, Error> {
        identity::encode(self)?;
        Ok(match self {
            Self::YouTubeMusic(id) => format!("https://music.youtube.com/watch?v={id}"),
            Self::YouTubeVideo(id) => format!("https://www.youtube.com/watch?v={id}"),
            Self::YouTubeShort(id) => format!("https://www.youtube.com/shorts/{id}"),
            Self::TikTokVideo(id) => format!("https://www.tiktok.com/@/video/{id}"),
            Self::TikTokShort(code) => format!("https://vt.tiktok.com/{code}/"),
            Self::InstagramReel(shortcode) => {
                format!("https://www.instagram.com/reel/{shortcode}/")
            }
            Self::Spotify(id) => format!("https://open.spotify.com/track/{id}"),
            Self::AppleMusic { id, storefront } => {
                format!("https://music.apple.com/{storefront}/song/{id}")
            }
            Self::Deezer(id) => format!("https://www.deezer.com/track/{id}"),
            Self::Tidal(id) => format!("https://tidal.com/browse/track/{id}"),
            Self::Qobuz(id) => format!("https://open.qobuz.com/track/{id}"),
            Self::AmazonMusic(asin) => format!("https://music.amazon.com/tracks/{asin}"),
        })
    }

    pub(crate) fn supports_moment(&self) -> bool {
        !matches!(self, Self::TikTokVideo(_) | Self::TikTokShort(_) | Self::InstagramReel(_))
    }
}

fn parameter<'a>(query: &'a str, key: &str) -> Option<&'a str> {
    query.split('&').find_map(|part| {
        let (name, value) = part.split_once('=')?;
        (name == key).then_some(value)
    })
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct MusicCode {
    pub track: Track,
    pub moment_seconds: Option<u16>,
}

impl MusicCode {
    pub fn from_url(url: &str) -> Result<Self, Error> {
        let track = Track::from_url(url)?;
        let mut moment_seconds = None;
        let without_fragment = url.split('#').next().unwrap_or(url);
        if let Some((_, query)) = without_fragment.split_once('?') {
            for parameter in query.split('&') {
                if let Some(("t", seconds)) = parameter.split_once('=') {
                    if moment_seconds.is_some() {
                        return Err(Error::InvalidTimestamp);
                    }
                    let seconds = seconds
                        .parse::<u16>()
                        .map_err(|_| Error::InvalidTimestamp)?;
                    moment_seconds = Some(seconds);
                }
            }
        }
        let code = Self {
            track,
            moment_seconds,
        };
        code.validate_moment()?;
        Ok(code)
    }

    pub(crate) fn validate_moment(&self) -> Result<(), Error> {
        if self.moment_seconds.is_some() && !self.track.supports_moment() {
            return Err(Error::InvalidTimestamp);
        }
        Ok(())
    }

    pub fn to_url(&self) -> Result<String, Error> {
        self.validate_moment()?;
        let mut url = match (&self.track, self.moment_seconds) {
            (Track::YouTubeShort(id), Some(_)) => {
                format!("https://www.youtube.com/watch?v={id}")
            }
            _ => self.track.to_url()?,
        };
        if let Some(seconds) = self.moment_seconds {
            if url.contains('?') {
                url.push_str("&t=");
            } else {
                url.push_str("?t=");
            }
            url.push_str(&seconds.to_string());
        }
        Ok(url)
    }
}

#[derive(Clone, Debug, Eq, PartialEq)]
pub struct Decoded {
    pub code: MusicCode,
    pub corrected_symbols: usize,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum Error {
    InvalidIdentifier,
    InvalidStorefront,
    UnsupportedUrl,
    InvalidLength,
    InvalidBar,
    InvalidCodeword,
    Uncorrectable,
    InvalidErasure,
    AmbiguousOrientation,
    InvalidTimestamp,
    ChecksumMismatch,
    UnsupportedVersion,
    InvalidFrame,
    MultipleCodes,
    InvalidGeometry,
    InvalidObservation,
}

impl std::fmt::Display for Error {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        let message = match self {
            Self::InvalidIdentifier => "invalid track identifier",
            Self::InvalidStorefront => "invalid Apple Music storefront",
            Self::UnsupportedUrl => "unsupported track URL",
            Self::InvalidLength => "unsupported WaveCode length",
            Self::InvalidBar => "bar level must be between 0 and 15",
            Self::InvalidCodeword => "reserved or invalid WaveCode payload",
            Self::Uncorrectable => "unable to correct WaveCode errors",
            Self::InvalidErasure => "invalid or repeated erasure position",
            Self::AmbiguousOrientation => "ambiguous WaveCode orientation",
            Self::InvalidTimestamp => "invalid timestamp, expected 0..65535 seconds",
            Self::ChecksumMismatch => "WaveCode checksum mismatch",
            Self::UnsupportedVersion => "unsupported WaveCode version",
            Self::InvalidFrame => "invalid WaveCode optical frame",
            Self::MultipleCodes => "multiple WaveCodes detected; use scan-all",
            Self::InvalidGeometry => "invalid WaveCode render dimensions or relative geometry",
            Self::InvalidObservation => "invalid or duplicate symbol hypothesis",
        };
        formatter.write_str(message)
    }
}

impl std::error::Error for Error {}

pub fn encode_symbols(code: &MusicCode) -> Result<Vec<u8>, Error> {
    code.validate_moment()?;
    let mut value = identity::encode(&code.track)?;
    let data_length = match code.moment_seconds {
        None => TRACK_SYMBOLS - PARITY_SYMBOLS,
        Some(moment @ 0..=4095) => {
            value = value.mul_small(4096).add(Rank::from_u64(moment as u64));
            SHORT_MOMENT_SYMBOLS - PARITY_SYMBOLS
        }
        Some(moment) => {
            value = value.mul_small(65536).add(Rank::from_u64(moment as u64));
            LONG_MOMENT_SYMBOLS - PARITY_SYMBOLS
        }
    };
    let mut data = value.to_bytes(data_length).ok_or(Error::InvalidCodeword)?;
    scrambling::apply(&mut data);
    Ok(reed_solomon::encode(&data))
}

pub fn decode_symbols(symbols: &[u8]) -> Result<Decoded, Error> {
    decode_symbols_with_erasures(symbols, &[])
}

pub fn decode_symbols_with_erasures(symbols: &[u8], erasures: &[usize]) -> Result<Decoded, Error> {
    let data_length = match symbols.len() {
        TRACK_SYMBOLS | SHORT_MOMENT_SYMBOLS | LONG_MOMENT_SYMBOLS => {
            symbols.len() - PARITY_SYMBOLS
        }
        _ => return Err(Error::InvalidLength),
    };
    let (corrected, corrected_symbols) = reed_solomon::decode_with_erasures(symbols, erasures)?;
    let mut payload = corrected[..data_length].to_vec();
    scrambling::apply(&mut payload);
    let value = Rank::from_bytes(&payload);
    let (identity, moment_seconds) = match symbols.len() {
        TRACK_SYMBOLS => (value, None),
        SHORT_MOMENT_SYMBOLS => {
            let (identity, moment) = value.div_mod_small(4096);
            (identity, Some(moment as u16))
        }
        LONG_MOMENT_SYMBOLS => {
            let (identity, moment) = value.div_mod_small(65536);
            if moment < 4096 {
                return Err(Error::InvalidCodeword);
            }
            (identity, Some(moment as u16))
        }
        _ => unreachable!(),
    };

    if identity >= identity::capacity() {
        return Err(Error::InvalidCodeword);
    }
    let code = MusicCode {
        track: identity::decode(identity)?,
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

pub fn encode(code: &MusicCode) -> Result<Vec<Bar>, Error> {
    Ok(bars_from_symbols(&encode_symbols(code)?))
}

pub fn decode(bars: &[Bar]) -> Result<Decoded, Error> {
    decode_symbols(&symbols_from_bars(bars)?)
}

pub fn decode_with_erasures(bars: &[Bar], erasures: &[usize]) -> Result<Decoded, Error> {
    decode_symbols_with_erasures(&symbols_from_bars(bars)?, erasures)
}

pub fn decode_oriented(bars: &[Bar]) -> Result<OrientedDecoded, Error> {
    let rotated: Vec<Bar> = bars
        .iter()
        .rev()
        .map(|bar| Bar {
            upper_level: bar.lower_level,
            lower_level: bar.upper_level,
        })
        .collect();

    match (decode(bars), decode(&rotated)) {
        (Ok(decoded), Err(_)) => Ok(OrientedDecoded {
            decoded,
            orientation: Orientation::Upright,
        }),
        (Err(_), Ok(decoded)) => Ok(OrientedDecoded {
            decoded,
            orientation: Orientation::UpsideDown,
        }),
        (Ok(_), Ok(_)) => Err(Error::AmbiguousOrientation),
        (Err(error), Err(_)) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, MusicCode, Track};

    #[test]
    fn full_round_trip() {
        let inputs = [
            MusicCode {
                track: Track::YouTubeMusic("dQw4w9WgXcQ".into()),
                moment_seconds: None,
            },
            MusicCode {
                track: Track::Spotify("0VjIjW4GlUZAMYd2vXMi3b".into()),
                moment_seconds: Some(254),
            },
            MusicCode {
                track: Track::AppleMusic {
                    id: u64::MAX,
                    storefront: "ua".into(),
                },
                moment_seconds: Some(u16::MAX),
            },
        ];
        for original in inputs {
            let bars = encode(&original).unwrap();
            assert_eq!(decode(&bars).unwrap().code, original);
        }
    }
}
