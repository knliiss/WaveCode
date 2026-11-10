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

fn locator(position: usize) -> u8 {
    power(2, (TOTAL_SYMBOLS - 1 - position) as u8)
}

fn erasure_annihilator(erasures: &[usize]) -> Vec<u8> {
    let mut polynomial = vec![1_u8];
    for &position in erasures {
        let x = locator(position);
        let mut next = vec![0_u8; polynomial.len() + 1];
        for (index, &coefficient) in polynomial.iter().enumerate() {
            next[index] ^= multiply(coefficient, x);
            next[index + 1] ^= coefficient;
        }
        polynomial = next;
    }
    polynomial
}

fn modified_syndromes(
    observed: &[u8; PARITY_SYMBOLS],
    erasures: &[usize],
) -> Vec<u8> {
    let polynomial = erasure_annihilator(erasures);
    (0..PARITY_SYMBOLS - erasures.len())
        .map(|index| {
            polynomial
                .iter()
                .enumerate()
                .fold(0_u8, |sum, (offset, coefficient)| {
                    sum ^ multiply(*coefficient, observed[index + offset])
                })
        })
        .collect()
}

fn berlekamp_massey(sequence: &[u8]) -> (Vec<u8>, usize) {
    let mut locator = vec![0_u8; sequence.len() + 1];
    let mut previous = locator.clone();
    locator[0] = 1;
    previous[0] = 1;

    let mut degree = 0;
    let mut shift = 1;
    let mut old_discrepancy = 1_u8;

    for index in 0..sequence.len() {
        let mut discrepancy = sequence[index];
        for j in 1..=degree {
            discrepancy ^= multiply(locator[j], sequence[index - j]);
        }

        if discrepancy == 0 {
            shift += 1;
            continue;
        }

        let saved = locator.clone();
        let factor = divide(discrepancy, old_discrepancy);
        for (j, &value) in previous.iter().enumerate().take(sequence.len() + 1 - shift) {
            locator[j + shift] ^= multiply(factor, value);
        }

        if 2 * degree <= index {
            degree = index + 1 - degree;
            previous = saved;
            old_discrepancy = discrepancy;
            shift = 1;
        } else {
            shift += 1;
        }
    }

    (locator, degree)
}

fn evaluate(polynomial: &[u8], x: u8) -> u8 {
    polynomial
        .iter()
        .rev()
        .fold(0_u8, |acc, coefficient| multiply(acc, x) ^ coefficient)
}

fn correct_at_positions(
    symbols: &[u8; TOTAL_SYMBOLS],
    observed: &[u8; PARITY_SYMBOLS],
    positions: &[usize],
) -> Option<([u8; TOTAL_SYMBOLS], usize)> {
    let count = positions.len();
    if count == 0 || count > PARITY_SYMBOLS {
        return None;
    }

    let mut matrix = [[0_u8; PARITY_SYMBOLS + 1]; PARITY_SYMBOLS];
    for row in 0..count {
        for (column, &position) in positions.iter().enumerate() {
            matrix[row][column] = power(locator(position), row as u8);
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
            factor = multiply(factor, locator(position));
        }
    }
    if &expected != observed {
        return None;
    }

    let mut repaired = *symbols;
    let mut changed = 0;
    for (index, &position) in positions.iter().enumerate() {
        let magnitude = matrix[index][count];
        if magnitude != 0 {
            repaired[position] ^= magnitude;
            changed += 1;
        }
    }
    Some((repaired, changed))
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
    if observed == [0; PARITY_SYMBOLS] {
        return Ok((received, 0));
    }

    let sequence = modified_syndromes(&observed, erasures);
    let (locator_polynomial, unknown) = berlekamp_massey(&sequence);
    if 2 * unknown + erasures.len() > PARITY_SYMBOLS {
        return Err(Error::Uncorrectable);
    }

    let mut locations = erasures.to_vec();
    for position in 0..TOTAL_SYMBOLS {
        if erasures.contains(&position) {
            continue;
        }
        let inverse = power(locator(position), 62);
        if evaluate(&locator_polynomial[..=unknown], inverse) == 0 {
            locations.push(position);
        }
    }
    if locations.len() != unknown + erasures.len() {
        return Err(Error::Uncorrectable);
    }

    correct_at_positions(&received, &observed, &locations).ok_or(Error::Uncorrectable)
}

#[cfg(test)]
mod tests {
    use super::{decode, encode, multiply, DATA_SYMBOLS, TOTAL_SYMBOLS};
    use crate::Error;

    #[test]
    fn primitive_generator_visits_every_nonzero_field_element() {
        let mut current = 1_u8;
        let mut visited = [false; 64];
        for _ in 0..63 {
            assert!(!visited[current as usize]);
            visited[current as usize] = true;
            current = multiply(current, 2);
        }
        assert_eq!(current, 1);
        assert!(visited[1..].iter().all(|value| *value));
    }

    #[test]
    fn exhaustive_triples_on_a_fixed_word() {
        let data = std::array::from_fn(|index| (index as u8 * 7) & 63);
        let expected = encode(&data);
        for a in 0..TOTAL_SYMBOLS {
            for b in a + 1..TOTAL_SYMBOLS {
                for c in b + 1..TOTAL_SYMBOLS {
                    let mut damaged = expected;
                    damaged[a] ^= 1;
                    damaged[b] ^= 37;
                    damaged[c] ^= 54;
                    assert_eq!(decode(&damaged, &[]).unwrap(), (expected, 3));
                }
            }
        }
    }

    #[test]
    fn mixed_known_and_unknown_corruption() {
        let expected = encode(&[19; DATA_SYMBOLS]);
        for (known, unknown) in [
            (vec![0, 4, 9, 15, 24, 33], vec![]),
            (vec![0, 4, 9, 15], vec![29]),
            (vec![0, 4], vec![11, 29]),
            (vec![], vec![8, 15, 32]),
        ] {
            let mut damaged = expected;
            for (index, &position) in known.iter().chain(&unknown).enumerate() {
                damaged[position] ^= (index as u8 * 7 + 1) & 63;
            }
            let (restored, count) = decode(&damaged, &known).unwrap();
            assert_eq!(restored, expected);
            assert_eq!(count, known.len() + unknown.len());
        }
    }

    #[test]
    fn unchanged_data_and_bad_erasures() {
        let expected = encode(&[0; DATA_SYMBOLS]);
        assert_eq!(decode(&expected, &[]).unwrap(), (expected, 0));
        assert_eq!(decode(&expected, &[2, 2]), Err(Error::InvalidErasure));
        assert_eq!(decode(&expected, &[TOTAL_SYMBOLS]), Err(Error::InvalidErasure));
        let mut corrupted = expected;
        corrupted[0] = 64;
        assert_eq!(decode(&corrupted, &[]), Err(Error::InvalidCodeword));
    }
}
