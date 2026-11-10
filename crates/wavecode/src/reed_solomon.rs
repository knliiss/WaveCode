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

fn correct_at_positions(
    symbols: &[u8],
    observed: &[u8; PARITY_SYMBOLS],
    positions: &[usize],
    locators: &[u8],
) -> Option<(Vec<u8>, usize)> {
    let size = positions.len();
    if size == 0 || size > PARITY_SYMBOLS {
        return None;
    }

    let mut matrix = [[0_u8; PARITY_SYMBOLS + 1]; PARITY_SYMBOLS];
    for row in 0..size {
        for (column, &position) in positions.iter().enumerate() {
            matrix[row][column] = power(locators[position], row as u16);
        }
        matrix[row][size] = observed[row];
    }

    for column in 0..size {
        let pivot = (column..size).find(|&row| matrix[row][column] != 0)?;
        matrix.swap(column, pivot);
        let inverse = divide(1, matrix[column][column]);
        for value in &mut matrix[column][column..=size] {
            *value = multiply(*value, inverse);
        }
        let pivot_row = matrix[column];
        for (row_index, row_values) in matrix.iter_mut().enumerate().take(size) {
            if row_index == column {
                continue;
            }
            let factor = row_values[column];
            for (value, source) in row_values[column..=size]
                .iter_mut()
                .zip(pivot_row[column..=size].iter())
            {
                *value ^= multiply(factor, *source);
            }
        }
    }

    let mut expected = [0_u8; PARITY_SYMBOLS];
    for (index, &position) in positions.iter().enumerate() {
        let magnitude = matrix[index][size];
        let mut factor = 1_u8;
        for syndrome in &mut expected {
            *syndrome ^= multiply(magnitude, factor);
            factor = multiply(factor, locators[position]);
        }
    }
    if expected != *observed {
        return None;
    }

    let mut repaired = symbols.to_vec();
    let mut count = 0;
    for (index, &position) in positions.iter().enumerate() {
        let magnitude = matrix[index][size];
        if magnitude != 0 {
            repaired[position] ^= magnitude;
            count += 1;
        }
    }
    Some((repaired, count))
}

pub(crate) fn decode_with_erasures(
    symbols: &[u8],
    erasures: &[usize],
) -> Result<(Vec<u8>, usize), Error> {
    if symbols.len() <= PARITY_SYMBOLS || symbols.len() > 255 {
        return Err(Error::InvalidLength);
    }
    if erasures.len() > PARITY_SYMBOLS
        || erasures.iter().any(|&position| position >= symbols.len())
        || erasures
            .iter()
            .enumerate()
            .any(|(index, position)| erasures[..index].contains(position))
    {
        return Err(Error::InvalidErasure);
    }
    if erasures.is_empty() {
        return decode(symbols);
    }

    let observed = syndromes(symbols);
    if observed == [0; PARITY_SYMBOLS] {
        return Ok((symbols.to_vec(), 0));
    }

    let locators: Vec<u8> = (0..symbols.len())
        .map(|position| power(2, (symbols.len() - 1 - position) as u16))
        .collect();
    if let Some(result) = correct_at_positions(symbols, &observed, erasures, &locators) {
        return Ok(result);
    }

    if erasures.len() + 2 <= PARITY_SYMBOLS {
        let mut positions = erasures.to_vec();
        for position in 0..symbols.len() {
            if erasures.contains(&position) {
                continue;
            }
            positions.push(position);
            if let Some(result) = correct_at_positions(symbols, &observed, &positions, &locators) {
                return Ok(result);
            }
            positions.pop();
        }
    }

    Err(Error::Uncorrectable)
}

#[cfg(test)]
mod tests {
    use super::{decode, decode_with_erasures, encode, generator, is_valid};
    use crate::Error;

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
    fn corrects_four_erased_symbols() {
        let data: Vec<u8> = (0..17).map(|index| index * 11).collect();
        let expected = encode(&data);
        let locations = [0, 3, 12, 20];
        let mut damaged = expected.clone();
        for &position in &locations {
            damaged[position] ^= 0xe3;
        }
        assert_eq!(
            decode_with_erasures(&damaged, &locations).unwrap(),
            (expected, 4)
        );
    }

    #[test]
    fn combines_two_erasures_with_one_unknown_error() {
        let expected = encode(&[17; 17]);
        let mut damaged = expected.clone();
        for (position, change) in [(1, 0x3a), (8, 0x92), (18, 0xb4)] {
            damaged[position] ^= change;
        }
        assert_eq!(
            decode_with_erasures(&damaged, &[1, 18]).unwrap(),
            (expected, 3)
        );
    }

    #[test]
    fn rejects_invalid_erasure_indexes() {
        let encoded = encode(&[0; 17]);
        assert_eq!(
            decode_with_erasures(&encoded, &[3, 3]),
            Err(Error::InvalidErasure)
        );
        assert_eq!(
            decode_with_erasures(&encoded, &[21]),
            Err(Error::InvalidErasure)
        );
        assert_eq!(
            decode_with_erasures(&encoded, &[0, 1, 2, 3, 4]),
            Err(Error::InvalidErasure)
        );
    }

    #[test]
    fn clean_word_is_untouched() {
        let encoded = encode(&[3; 17]);
        assert_eq!(decode(&encoded).unwrap(), (encoded, 0));
    }
}
