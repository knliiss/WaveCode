use crate::Error;

pub(super) const DATA_SYMBOLS: usize = 28;
pub(super) const PARITY_SYMBOLS: usize = 6;
pub(super) const TOTAL_SYMBOLS: usize = DATA_SYMBOLS + PARITY_SYMBOLS;

fn multiply(mut left: u8, mut right: u8) -> u8 {
    let mut product = 0_u8;
    for _ in 0..6 {
        if right & 1 != 0 {
            product ^= left;
        }
        let overflow = left & 0x20 != 0;
        left = (left << 1) & 0x3f;
        if overflow {
            left ^= 0x03;
        }
        right >>= 1;
    }
    product
}

fn power(mut base: u8, mut exponent: u8) -> u8 {
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
    debug_assert_ne!(right, 0);
    multiply(left, power(right, 62))
}

fn generator() -> [u8; PARITY_SYMBOLS + 1] {
    let mut polynomial = [0_u8; PARITY_SYMBOLS + 1];
    polynomial[0] = 1;

    for degree in 0..PARITY_SYMBOLS {
        let root = power(2, degree as u8);
        for index in (1..=degree + 1).rev() {
            polynomial[index] ^= multiply(polynomial[index - 1], root);
        }
    }
    polynomial
}

pub(super) fn encode(data: &[u8; DATA_SYMBOLS]) -> [u8; TOTAL_SYMBOLS] {
    let mut remainder = [0_u8; TOTAL_SYMBOLS];
    remainder[..DATA_SYMBOLS].copy_from_slice(data);
    let factors = generator();

    for index in 0..DATA_SYMBOLS {
        let coefficient = remainder[index];
        for (offset, factor) in factors.iter().enumerate() {
            remainder[index + offset] ^= multiply(coefficient, *factor);
        }
    }

    let mut output = [0_u8; TOTAL_SYMBOLS];
    output[..DATA_SYMBOLS].copy_from_slice(data);
    output[DATA_SYMBOLS..].copy_from_slice(&remainder[DATA_SYMBOLS..]);
    output
}

fn syndromes(symbols: &[u8; TOTAL_SYMBOLS]) -> [u8; PARITY_SYMBOLS] {
    let mut result = [0_u8; PARITY_SYMBOLS];
    for (degree, syndrome) in result.iter_mut().enumerate() {
        let root = power(2, degree as u8);
        *syndrome = symbols
            .iter()
            .fold(0_u8, |value, symbol| multiply(value, root) ^ *symbol);
    }
    result
}

fn correct_at_positions(
    symbols: &[u8; TOTAL_SYMBOLS],
    observed: &[u8; PARITY_SYMBOLS],
    positions: &[usize],
    locators: &[u8; TOTAL_SYMBOLS],
) -> Option<([u8; TOTAL_SYMBOLS], usize)> {
    let count = positions.len();
    if count == 0 || count > PARITY_SYMBOLS {
        return None;
    }

    let mut matrix = [[0_u8; PARITY_SYMBOLS + 1]; PARITY_SYMBOLS];
    for row in 0..count {
        for (column, &position) in positions.iter().enumerate() {
            matrix[row][column] = power(locators[position], row as u8);
        }
        matrix[row][count] = observed[row];
    }

    for column in 0..count {
        let pivot = (column..count).find(|&row| matrix[row][column] != 0)?;
        matrix.swap(column, pivot);

        let inverse = divide(1, matrix[column][column]);
        for value in &mut matrix[column][column..=count] {
            *value = multiply(*value, inverse);
        }

        let pivot_row = matrix[column];
        for (index, row) in matrix.iter_mut().enumerate().take(count) {
            if index == column {
                continue;
            }
            let factor = row[column];
            for (value, source) in row[column..=count]
                .iter_mut()
                .zip(pivot_row[column..=count].iter())
            {
                *value ^= multiply(factor, *source);
            }
        }
    }

    let mut expected = [0_u8; PARITY_SYMBOLS];
    for (index, &position) in positions.iter().enumerate() {
        let magnitude = matrix[index][count];
        let mut factor = 1_u8;
        for syndrome in &mut expected {
            *syndrome ^= multiply(magnitude, factor);
            factor = multiply(factor, locators[position]);
        }
    }

    if &expected != observed {
        return None;
    }

    let mut repaired = *symbols;
    let mut changed = 0;
    for (index, &position) in positions.iter().enumerate() {
        let difference = matrix[index][count];
        if difference != 0 {
            repaired[position] ^= difference;
            changed += 1;
        }
    }

    Some((repaired, changed))
}

struct Search<'a> {
    original: &'a [u8; TOTAL_SYMBOLS],
    observed: &'a [u8; PARITY_SYMBOLS],
    erasures: &'a [usize],
    locators: &'a [u8; TOTAL_SYMBOLS],
}

impl Search<'_> {
    fn find(
        &self,
        positions: &mut Vec<usize>,
        next: usize,
        unknown_left: usize,
    ) -> Option<([u8; TOTAL_SYMBOLS], usize)> {
        if unknown_left == 0 {
            return correct_at_positions(
                self.original,
                self.observed,
                positions,
                self.locators,
            );
        }

        for candidate in next..TOTAL_SYMBOLS {
            if self.erasures.contains(&candidate) {
                continue;
            }

            positions.push(candidate);
            if let Some(result) = self.find(positions, candidate + 1, unknown_left - 1) {
                return Some(result);
            }
            positions.pop();
        }

        None
    }
}

pub(super) fn decode(
    symbols: &[u8],
    erasures: &[usize],
) -> Result<([u8; TOTAL_SYMBOLS], usize), Error> {
    let received: [u8; TOTAL_SYMBOLS] = symbols.try_into().map_err(|_| Error::InvalidLength)?;
    if received.iter().any(|symbol| *symbol >= 64) {
        return Err(Error::InvalidCodeword);
    }
    if erasures.len() > PARITY_SYMBOLS
        || erasures.iter().any(|&position| position >= TOTAL_SYMBOLS)
        || erasures
            .iter()
            .enumerate()
            .any(|(index, position)| erasures[..index].contains(position))
    {
        return Err(Error::InvalidErasure);
    }

    let observed = syndromes(&received);
    if observed == [0_u8; PARITY_SYMBOLS] {
        return Ok((received, 0));
    }

    let locators = std::array::from_fn(|position| {
        power(2, (TOTAL_SYMBOLS - 1 - position) as u8)
    });
    let search = Search {
        original: &received,
        observed: &observed,
        erasures,
        locators: &locators,
    };

    for unknown in 0..=(PARITY_SYMBOLS - erasures.len()) / 2 {
        let mut positions = erasures.to_vec();
        if let Some(result) = search.find(&mut positions, 0, unknown) {
            return Ok(result);
        }
    }

    Err(Error::Uncorrectable)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, multiply, DATA_SYMBOLS, TOTAL_SYMBOLS};
    use crate::Error;

    #[test]
    fn generator_is_primitive_over_gf64() {
        let mut power = 1_u8;
        let mut visited = [false; 64];
        for _ in 0..63 {
            assert!(!visited[power as usize]);
            visited[power as usize] = true;
            power = multiply(power, 2);
        }
        assert_eq!(power, 1);
        assert!(visited[1..].iter().all(|seen| *seen));
    }

    #[test]
    fn corrects_three_unknown_errors() {
        let data = std::array::from_fn(|index| (index as u8 * 7) & 63);
        let expected = encode(&data);
        for first in [0, 7, 15, 29] {
            for second in first + 1..TOTAL_SYMBOLS {
                let third = TOTAL_SYMBOLS - 1;
                if second >= third {
                    continue;
                }
                let mut damaged = expected;
                damaged[first] ^= 1;
                damaged[second] ^= 37;
                damaged[third] ^= 54;
                assert_eq!(decode(&damaged, &[]).unwrap(), (expected, 3));
            }
        }
    }

    #[test]
    fn corrects_six_erasures() {
        let data = [19_u8; DATA_SYMBOLS];
        let expected = encode(&data);
        let known = [0, 4, 9, 15, 24, 33];
        let mut damaged = expected;
        for index in known {
            damaged[index] ^= 47;
        }
        assert_eq!(decode(&damaged, &known).unwrap(), (expected, 6));
    }

    #[test]
    fn corrects_mixed_errors_and_erasures() {
        let expected = encode(&[3; DATA_SYMBOLS]);
        for known in [[0, 4, 17, 32].as_slice(), [5, 23].as_slice()] {
            let mut damaged = expected;
            for position in known {
                damaged[*position] ^= 43;
            }
            if known.len() == 4 {
                damaged[11] ^= 27;
            } else {
                damaged[11] ^= 27;
                damaged[30] ^= 21;
            }
            let result = decode(&damaged, known).unwrap();
            assert_eq!(result.0, expected);
        }
    }

    #[test]
    fn rejects_invalid_symbols_and_erasure_indexes() {
        let valid = encode(&[0; DATA_SYMBOLS]);
        let mut invalid = valid;
        invalid[0] = 64;
        assert_eq!(decode(&invalid, &[]), Err(Error::InvalidCodeword));
        assert_eq!(decode(&valid, &[0, 0]), Err(Error::InvalidErasure));
        assert_eq!(decode(&valid, &[TOTAL_SYMBOLS]), Err(Error::InvalidErasure));
    }
}
