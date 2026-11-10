use crate::rank::Rank;
use crate::{Error, Track};

const YOUTUBE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const SPOTIFY_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
const AMAZON_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZ";

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

pub(crate) fn capacity() -> Rank {
    amazon_start().add(amazon_space())
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
        Track::YouTubeMusic(id) => {
            if id.len() != 11 {
                return Err(Error::InvalidIdentifier);
            }
            Rank::from_digits(id, YOUTUBE_ALPHABET).ok_or(Error::InvalidIdentifier)
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
            let number = Rank::from_digits(asin, AMAZON_ALPHABET)
                .ok_or(Error::InvalidIdentifier)?;
            Ok(amazon_start().add(number))
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
    if number >= capacity() {
        return Err(Error::InvalidCodeword);
    }
    let asin = number
        .sub(amazon_start())
        .to_digits(AMAZON_ALPHABET, 10)
        .ok_or(Error::InvalidCodeword)?;
    Ok(Track::AmazonMusic(asin))
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
