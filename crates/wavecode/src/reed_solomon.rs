use crate::Error;

pub(crate) const PARITY_SYMBOLS: usize = 4;

fn multiply(mut left: u8, mut right: u8) -> u8 {
    let mut product = 0_u8;
    for _ in 0..8 {
        if right & 1 != 0 {
            product ^= left;
        }
        let overflow = left & 0x80 != 0;
        left <<= 1;
        if overflow {
            left ^= 0x1d;
        }
        right >>= 1;
    }
    product
}

fn power(mut base: u8, mut exponent: u16) -> u8 {
    let mut result = 1_u8;
    while exponent > 0 {
        if exponent & 1 != 0 {
            result = multiply(result, base);
        }
        base = multiply(base, base);
        exponent >>= 1;
    }
    result
}

fn divide(left: u8, right: u8) -> u8 {
    assert_ne!(right, 0);
    multiply(left, power(right, 254))
}

fn generator() -> [u8; PARITY_SYMBOLS + 1] {
    let mut polynomial = [0_u8; PARITY_SYMBOLS + 1];
    polynomial[0] = 1;
    for degree in 0..PARITY_SYMBOLS {
        let root = power(2, degree as u16);
        for index in (1..=degree + 1).rev() {
            polynomial[index] ^= multiply(polynomial[index - 1], root);
        }
    }
    polynomial
}

fn syndromes(symbols: &[u8]) -> [u8; PARITY_SYMBOLS] {
    let mut result = [0_u8; PARITY_SYMBOLS];
    for (degree, syndrome) in result.iter_mut().enumerate() {
        let root = power(2, degree as u16);
        *syndrome = symbols
            .iter()
            .fold(0_u8, |acc, symbol| multiply(acc, root) ^ *symbol);
    }
    result
}

fn is_valid(symbols: &[u8]) -> bool {
    syndromes(symbols).iter().all(|syndrome| *syndrome == 0)
}

pub(crate) fn encode(data: &[u8]) -> Vec<u8> {
    let mut polynomial = vec![0_u8; data.len() + PARITY_SYMBOLS];
    polynomial[..data.len()].copy_from_slice(data);
    let factors = generator();

    for index in 0..data.len() {
        let coefficient = polynomial[index];
        if coefficient != 0 {
            for (offset, factor) in factors.iter().enumerate() {
                polynomial[index + offset] ^= multiply(coefficient, *factor);
            }
        }
    }

    let mut result = data.to_vec();
    result.extend_from_slice(&polynomial[data.len()..]);
    result
}

pub(crate) fn decode(symbols: &[u8]) -> Result<(Vec<u8>, usize), Error> {
    if symbols.len() <= PARITY_SYMBOLS || symbols.len() > 255 {
        return Err(Error::InvalidLength);
    }
    let mut corrected = symbols.to_vec();
    let observed = syndromes(symbols);
    if observed == [0; PARITY_SYMBOLS] {
        return Ok((corrected, 0));
    }

    for index in 0..symbols.len() {
        let error = observed[0];
        if error == 0 {
            break;
        }
        corrected[index] ^= error;
        if is_valid(&corrected) {
            return Ok((corrected, 1));
        }
        corrected[index] ^= error;
    }

    for first in 0..symbols.len() {
        let x = power(2, (symbols.len() - 1 - first) as u16);
        for second in first + 1..symbols.len() {
            let y = power(2, (symbols.len() - 1 - second) as u16);
            let second_error = divide(observed[1] ^ multiply(observed[0], x), x ^ y);
            let first_error = observed[0] ^ second_error;
            if first_error == 0 || second_error == 0 {
                continue;
            }
            corrected[first] ^= first_error;
            corrected[second] ^= second_error;
            if is_valid(&corrected) {
                return Ok((corrected, 2));
            }
            corrected[first] ^= first_error;
            corrected[second] ^= second_error;
        }
    }

    Err(Error::Uncorrectable)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, generator, is_valid};

    #[test]
    fn generator_has_four_distinct_roots() {
        assert_eq!(generator()[0], 1);
        for input in [vec![0; 17], vec![255; 17], (0..17).collect()] {
            assert!(is_valid(&encode(&input)));
        }
    }

    #[test]
    fn corrects_each_pair_of_symbol_errors() {
        let input: Vec<u8> = (0..19).map(|i| (i * 13 + 7) as u8).collect();
        let encoded = encode(&input);
        for first in 0..encoded.len() {
            for second in first + 1..encoded.len() {
                let mut corrupted = encoded.clone();
                corrupted[first] ^= 0x5b;
                corrupted[second] ^= 0xc6;
                let (repaired, count) = decode(&corrupted).unwrap();
                assert_eq!(count, 2);
                assert_eq!(repaired, encoded);
            }
        }
    }

    #[test]
    fn clean_word_is_untouched() {
        let encoded = encode(&[3; 17]);
        assert_eq!(decode(&encoded).unwrap(), (encoded, 0));
    }
}
