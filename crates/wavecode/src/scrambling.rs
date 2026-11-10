const PAYLOAD_MASK: [u8; 19] = [
    0x5b, 0x03, 0x53, 0x34, 0x06, 0xb4, 0x6d, 0x37, 0x7c, 0xb5, 0xab, 0x0d, 0x3c, 0x07, 0xf8, 0x9f,
    0x95, 0x62, 0xfb,
];

pub(crate) fn apply(payload: &mut [u8]) {
    for (byte, mask) in payload.iter_mut().zip(PAYLOAD_MASK) {
        *byte ^= mask;
    }
}

#[cfg(test)]
mod tests {
    use super::apply;

    #[test]
    fn scrambling_is_an_involution() {
        for length in [17, 18, 19] {
            let original = vec![0_u8; length];
            let mut scrambled = original.clone();
            apply(&mut scrambled);
            assert_ne!(scrambled, original);
            apply(&mut scrambled);
            assert_eq!(scrambled, original);
        }
    }
}
