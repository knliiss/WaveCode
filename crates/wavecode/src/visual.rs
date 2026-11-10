use crate::Error;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct Bar {
    pub upper_level: u8,
    pub lower_level: u8,
}

impl Bar {
    pub fn from_symbol(symbol: u8) -> Self {
        Self {
            upper_level: symbol >> 4,
            lower_level: symbol & 0x0f,
        }
    }

    pub fn symbol(self) -> Result<u8, Error> {
        if self.upper_level > 15 || self.lower_level > 15 {
            return Err(Error::InvalidBar);
        }
        Ok((self.upper_level << 4) | self.lower_level)
    }

    pub fn heights(self) -> Result<(u8, u8), Error> {
        self.symbol()?;
        Ok((self.upper_level + 1, self.lower_level + 1))
    }
}

pub fn from_symbols(symbols: &[u8]) -> Vec<Bar> {
    symbols.iter().copied().map(Bar::from_symbol).collect()
}

pub fn to_symbols(bars: &[Bar]) -> Result<Vec<u8>, Error> {
    bars.iter().map(|bar| bar.symbol()).collect()
}

#[cfg(test)]
mod tests {
    use super::Bar;

    #[test]
    fn all_symbol_values_have_unique_geometry() {
        for symbol in 0_u8..=255 {
            let bar = Bar::from_symbol(symbol);
            assert_eq!(bar.symbol().unwrap(), symbol);
            assert!((1..=16).contains(&bar.heights().unwrap().0));
        }
    }
}
