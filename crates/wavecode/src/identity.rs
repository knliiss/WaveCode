use crate::rank::Rank;
use crate::{Error, Track};

const YOUTUBE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const SPOTIFY_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const AMAZON_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const INSTAGRAM_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const MAX_INSTAGRAM_SHORTCODE: usize = 16;

fn youtube_space() -> Rank {
    Rank::pow(64, 11)
}

fn apple_space() -> Rank {
    Rank::from_limbs(0, 676)
}

fn spotify_space() -> Rank {
    Rank::pow(62, 22)
}

fn legacy_capacity() -> Rank {
    youtube_space().add(apple_space()).add(spotify_space())
}

fn numeric_space() -> Rank {
    Rank::from_limbs(0, 1)
}

fn tidal_start() -> Rank {
    legacy_capacity().add(numeric_space())
}

fn qobuz_start() -> Rank {
    tidal_start().add(numeric_space())
}

fn amazon_start() -> Rank {
    qobuz_start().add(numeric_space())
}

fn amazon_space() -> Rank {
    Rank::pow(36, 10)
}

fn youtube_video_start() -> Rank {
    amazon_start().add(amazon_space())
}

fn youtube_shorts_start() -> Rank {
    youtube_video_start().add(youtube_space())
}

fn tiktok_start() -> Rank {
    youtube_shorts_start().add(youtube_space())
}

fn instagram_start() -> Rank {
    tiktok_start().add(Rank::pow(2, 64))
}

fn instagram_space() -> Rank {
    (1..=MAX_INSTAGRAM_SHORTCODE)
        .fold(Rank::zero(), |sum, length| sum.add(Rank::pow(64, length)))
}

fn instagram_length_start(length: usize) -> Rank {
    (1..length).fold(Rank::zero(), |sum, n| sum.add(Rank::pow(64, n)))
}

pub(crate) fn capacity() -> Rank {
    instagram_start().add(instagram_space())
}

fn storefront_rank(storefront: &str) -> Result<u64, Error> {
    let bytes = storefront.as_bytes();
    if bytes.len() != 2 || !bytes.iter().all(|byte| byte.is_ascii_lowercase()) {
        return Err(Error::InvalidStorefront);
    }
    Ok(((bytes[0] - b'a') as u64) * 26 + (bytes[1] - b'a') as u64)
}

pub(crate) fn encode(track: &Track) -> Result<Rank, Error> {
    match track {
        Track::YouTubeMusic(id) | Track::YouTubeVideo(id) | Track::YouTubeShort(id) => {
            if id.len() != 11 {
                return Err(Error::InvalidIdentifier);
            }
            let rank = Rank::from_digits(id, YOUTUBE_ALPHABET).ok_or(Error::InvalidIdentifier)?;
            match track {
                Track::YouTubeMusic(_) => Ok(rank),
                Track::YouTubeVideo(_) => Ok(youtube_video_start().add(rank)),
                Track::YouTubeShort(_) => Ok(youtube_shorts_start().add(rank)),
                _ => unreachable!(),
            }
        }
        Track::Spotify(id) => {
            if id.len() != 22 {
                return Err(Error::InvalidIdentifier);
            }
            let number = Rank::from_digits(id, SPOTIFY_ALPHABET).ok_or(Error::InvalidIdentifier)?;
            Ok(youtube_space().add(apple_space()).add(number))
        }
        Track::AppleMusic { id, storefront } => {
            let storefront = storefront_rank(storefront)?;
            let value = Rank::from_limbs(*id, storefront);
            Ok(youtube_space().add(value))
        }
        Track::Deezer(id) => Ok(legacy_capacity().add(Rank::from_u64(*id))),
        Track::Tidal(id) => Ok(tidal_start().add(Rank::from_u64(*id))),
        Track::Qobuz(id) => Ok(qobuz_start().add(Rank::from_u64(*id))),
        Track::AmazonMusic(asin) => {
            if asin.len() != 10 {
                return Err(Error::InvalidIdentifier);
            }
            let number =
                Rank::from_digits(asin, AMAZON_ALPHABET).ok_or(Error::InvalidIdentifier)?;
            Ok(amazon_start().add(number))
        }
        Track::TikTokVideo(id) => Ok(tiktok_start().add(Rank::from_u64(*id))),
        Track::InstagramReel(shortcode) => {
            if shortcode.is_empty() || shortcode.len() > MAX_INSTAGRAM_SHORTCODE {
                return Err(Error::InvalidIdentifier);
            }
            let number = Rank::from_digits(shortcode, INSTAGRAM_ALPHABET)
                .ok_or(Error::InvalidIdentifier)?;
            Ok(instagram_start()
                .add(instagram_length_start(shortcode.len()))
                .add(number))
        }
    }
}

pub(crate) fn decode(number: Rank) -> Result<Track, Error> {
    let youtube_end = youtube_space();
    if number < youtube_end {
        return Ok(Track::YouTubeMusic(
            number
                .to_digits(YOUTUBE_ALPHABET, 11)
                .ok_or(Error::InvalidCodeword)?,
        ));
    }

    let apple_end = youtube_end.add(apple_space());
    if number < apple_end {
        let apple = number.sub(youtube_end);
        let index = apple.middle_u64();
        let country = format!(
            "{}{}",
            (b'a' + (index / 26) as u8) as char,
            (b'a' + (index % 26) as u8) as char,
        );
        let song_id = apple.low_u64();
        return Ok(Track::AppleMusic {
            id: song_id,
            storefront: country,
        });
    }

    if number < legacy_capacity() {
        let spotify = number
            .sub(apple_end)
            .to_digits(SPOTIFY_ALPHABET, 22)
            .ok_or(Error::InvalidCodeword)?;
        return Ok(Track::Spotify(spotify));
    }

    if number < tidal_start() {
        return Ok(Track::Deezer(number.sub(legacy_capacity()).low_u64()));
    }
    if number < qobuz_start() {
        return Ok(Track::Tidal(number.sub(tidal_start()).low_u64()));
    }
    if number < amazon_start() {
        return Ok(Track::Qobuz(number.sub(qobuz_start()).low_u64()));
    }
    if number < youtube_video_start() {
        let asin = number
            .sub(amazon_start())
            .to_digits(AMAZON_ALPHABET, 10)
            .ok_or(Error::InvalidCodeword)?;
        return Ok(Track::AmazonMusic(asin));
    }
    if number < youtube_shorts_start() {
        let id = number
            .sub(youtube_video_start())
            .to_digits(YOUTUBE_ALPHABET, 11)
            .ok_or(Error::InvalidCodeword)?;
        return Ok(Track::YouTubeVideo(id));
    }
    if number < tiktok_start() {
        let id = number
            .sub(youtube_shorts_start())
            .to_digits(YOUTUBE_ALPHABET, 11)
            .ok_or(Error::InvalidCodeword)?;
        return Ok(Track::YouTubeShort(id));
    }
    if number < instagram_start() {
        return Ok(Track::TikTokVideo(number.sub(tiktok_start()).low_u64()));
    }
    if number >= capacity() {
        return Err(Error::InvalidCodeword);
    }

    let mut remaining = number.sub(instagram_start());
    for length in 1..=MAX_INSTAGRAM_SHORTCODE {
        let space = Rank::pow(64, length);
        if remaining < space {
            let shortcode = remaining
                .to_digits(INSTAGRAM_ALPHABET, length)
                .ok_or(Error::InvalidCodeword)?;
            return Ok(Track::InstagramReel(shortcode));
        }
        remaining = remaining.sub(space);
    }
    Err(Error::InvalidCodeword)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode};
    use crate::Track;

    #[test]
    fn complete_spaces_are_reversible() {
        for track in [
            Track::YouTubeMusic("AAAAAAAAAAA".into()),
            Track::YouTubeMusic("___________".into()),
            Track::YouTubeVideo("AAAAAAAAAAA".into()),
            Track::YouTubeVideo("___________".into()),
            Track::YouTubeShort("AAAAAAAAAAA".into()),
            Track::YouTubeShort("___________".into()),
            Track::TikTokVideo(0),
            Track::TikTokVideo(u64::MAX),
            Track::InstagramReel("A".into()),
            Track::InstagramReel("C1234567890".into()),
            Track::InstagramReel("_______________".into()),
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
            Track::Deezer(0),
            Track::Deezer(u64::MAX),
            Track::Tidal(0),
            Track::Tidal(u64::MAX),
            Track::Qobuz(0),
            Track::Qobuz(u64::MAX),
            Track::AmazonMusic("0000000000".into()),
            Track::AmazonMusic("ZZZZZZZZZZ".into()),
        ] {
            assert_eq!(decode(encode(&track).unwrap()).unwrap(), track);
        }
    }
}
