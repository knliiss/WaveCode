use std::cmp::Ordering;

#[derive(Clone, Copy, Debug, Default, Eq, PartialEq)]
pub(crate) struct Rank([u64; 3]);

impl Rank {
    pub(crate) const fn zero() -> Self {
        Self([0; 3])
    }

    pub(crate) const fn from_u64(value: u64) -> Self {
        Self([value, 0, 0])
    }

    pub(crate) const fn from_limbs(low: u64, middle: u64) -> Self {
        Self([low, middle, 0])
    }

    pub(crate) const fn low_u64(self) -> u64 {
        self.0[0]
    }

    pub(crate) const fn middle_u64(self) -> u64 {
        self.0[1]
    }

    pub(crate) fn add(self, other: Self) -> Self {
        let mut result = [0; 3];
        let mut carry = 0_u128;

        for (index, output) in result.iter_mut().enumerate() {
            let value = self.0[index] as u128 + other.0[index] as u128 + carry;
            *output = value as u64;
            carry = value >> 64;
        }

        assert_eq!(carry, 0, "rank overflow");
        Self(result)
    }

    pub(crate) fn sub(self, other: Self) -> Self {
        assert!(self >= other, "rank underflow");
        let mut result = [0; 3];
        let mut borrow = 0_u128;

        for (index, output) in result.iter_mut().enumerate() {
            let left = self.0[index] as u128;
            let right = other.0[index] as u128 + borrow;
            if left >= right {
                *output = (left - right) as u64;
                borrow = 0;
            } else {
                *output = ((1_u128 << 64) + left - right) as u64;
                borrow = 1;
            }
        }

        Self(result)
    }

    pub(crate) fn mul_small(self, value: u64) -> Self {
        let mut result = [0; 3];
        let mut carry = 0_u128;

        for (index, output) in result.iter_mut().enumerate() {
            let product = self.0[index] as u128 * value as u128 + carry;
            *output = product as u64;
            carry = product >> 64;
        }

        assert_eq!(carry, 0, "rank overflow");
        Self(result)
    }

    pub(crate) fn div_mod_small(self, divisor: u64) -> (Self, u64) {
        assert_ne!(divisor, 0);
        let mut result = [0; 3];
        let mut remainder = 0_u128;

        for index in (0..3).rev() {
            let value = (remainder << 64) | self.0[index] as u128;
            result[index] = (value / divisor as u128) as u64;
            remainder = value % divisor as u128;
        }

        (Self(result), remainder as u64)
    }

    pub(crate) fn pow(base: u64, exponent: usize) -> Self {
        (0..exponent).fold(Self::from_u64(1), |acc, _| acc.mul_small(base))
    }

    pub(crate) fn from_digits(text: &str, alphabet: &[u8]) -> Option<Self> {
        let mut number = Self::zero();
        for byte in text.bytes() {
            let digit = alphabet.iter().position(|candidate| *candidate == byte)? as u64;
            number = number
                .mul_small(alphabet.len() as u64)
                .add(Self::from_u64(digit));
        }
        Some(number)
    }

    pub(crate) fn to_digits(self, alphabet: &[u8], length: usize) -> Option<String> {
        let mut number = self;
        let mut output = vec![0_u8; length];
        for digit in output.iter_mut().rev() {
            let (quotient, remainder) = number.div_mod_small(alphabet.len() as u64);
            *digit = alphabet[remainder as usize];
            number = quotient;
        }
        (number == Self::zero()).then(|| String::from_utf8(output).expect("ASCII alphabet"))
    }

    pub(crate) fn to_bytes(self, length: usize) -> Option<Vec<u8>> {
        let mut number = self;
        let mut output = vec![0_u8; length];
        for byte in output.iter_mut().rev() {
            let (quotient, remainder) = number.div_mod_small(256);
            *byte = remainder as u8;
            number = quotient;
        }
        (number == Self::zero()).then_some(output)
    }

    pub(crate) fn from_bytes(bytes: &[u8]) -> Self {
        bytes.iter().fold(Self::zero(), |number, byte| {
            number.mul_small(256).add(Self::from_u64(*byte as u64))
        })
    }
}

impl Ord for Rank {
    fn cmp(&self, other: &Self) -> Ordering {
        for index in (0..3).rev() {
            match self.0[index].cmp(&other.0[index]) {
                Ordering::Equal => continue,
                result => return result,
            }
        }
        Ordering::Equal
    }
}

impl PartialOrd for Rank {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.cmp(other))
    }
}

#[cfg(test)]
mod tests {
    use super::Rank;

    #[test]
    fn carries_across_limbs() {
        let value = Rank::from_u64(u64::MAX).add(Rank::from_u64(1));
        assert_eq!(value, Rank::from_limbs(0, 1));
        assert_eq!(value.sub(Rank::from_u64(1)), Rank::from_u64(u64::MAX));
    }

    #[test]
    fn radix_conversion_preserves_leading_zeroes() {
        let alphabet = b"0123456789ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz";
        let input = "0000000000000000000001";
        let value = Rank::from_digits(input, alphabet).unwrap();
        assert_eq!(value.to_digits(alphabet, 22).unwrap(), input);
    }

    #[test]
    fn byte_conversion_preserves_high_values() {
        let number = Rank::pow(62, 22).sub(Rank::from_u64(1));
        let bytes = number.to_bytes(17).unwrap();
        assert_eq!(Rank::from_bytes(&bytes), number);
    }
}
