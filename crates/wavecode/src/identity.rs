use crate::rank::Rank;
use crate::{Error, Track};

const YOUTUBE_ALPHABET: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789-_";
const SPOTIFY_ALPHABET: &[u8] = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";

fn youtube_space() -> Rank {
    Rank::pow(64, 11)
}

fn apple_space() -> Rank {
    Rank::from_limbs(0, 676)
}

fn spotify_space() -> Rank {
    Rank::pow(62, 22)
}

pub(crate) fn capacity() -> Rank {
    youtube_space().add(apple_space()).add(spotify_space())
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
            let number = Rank::from_digits(id, SPOTIFY_ALPHABET)
                .ok_or(Error::InvalidIdentifier)?;
            Ok(youtube_space().add(apple_space()).add(number))
        }
        Track::AppleMusic { id, storefront } => {
            let storefront = storefront_rank(storefront)?;
            let value = Rank::from_limbs(*id, storefront);
            Ok(youtube_space().add(value))
        }
    }
}

pub(crate) fn decode(number: Rank) -> Result<Track, Error> {
    let youtube_end = youtube_space();
    if number < youtube_end {
        return Ok(Track::YouTubeMusic(
            number.to_digits(YOUTUBE_ALPHABET, 11).ok_or(Error::InvalidCodeword)?,
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

    if number >= capacity() {
        return Err(Error::InvalidCodeword);
    }
    let spotify = number.sub(apple_end)
        .to_digits(SPOTIFY_ALPHABET, 22)
        .ok_or(Error::InvalidCodeword)?;
    Ok(Track::Spotify(spotify))
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
            Track::AppleMusic { id: 0, storefront: "aa".into() },
            Track::AppleMusic { id: u64::MAX, storefront: "zz".into() },
        ] {
            assert_eq!(decode(encode(&track).unwrap()).unwrap(), track);
        }
    }
}
