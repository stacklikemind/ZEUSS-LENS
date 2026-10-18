use crate::{Error, Result};

pub fn push_number(bits: &mut Vec<u8>, value: u32, width: usize) {
    bits.extend((0..width).rev().map(|i| ((value >> i) & 1) as u8));
}

pub fn unpack(bytes: &[u8], width: usize) -> Vec<u8> {
    let mut bits = Vec::with_capacity(bytes.len() * width);
    for &byte in bytes {
        push_number(&mut bits, byte as u32, width);
    }
    bits
}

pub fn pack(bits: &[u8], width: usize) -> Vec<u8> {
    bits.chunks(width)
        .map(|chunk| chunk.iter().fold(0, |n, &bit| (n << 1) | bit))
        .collect()
}

pub fn parse_hex<const N: usize>(text: &str, field: &str) -> Result<[u8; N]> {
    if text.len() != N * 2 || !text.is_ascii() {
        return Err(Error::input(format!(
            "{field} must contain exactly {} hexadecimal characters",
            N * 2
        )));
    }
    let mut out = [0; N];
    for (i, pair) in text.as_bytes().chunks_exact(2).enumerate() {
        let digit = |c: u8| -> Option<u8> {
            match c {
                b'0'..=b'9' => Some(c - b'0'),
                b'a'..=b'f' => Some(c - b'a' + 10),
                b'A'..=b'F' => Some(c - b'A' + 10),
                _ => None,
            }
        };
        out[i] = digit(pair[0])
            .zip(digit(pair[1]))
            .map(|(a, b)| a * 16 + b)
            .ok_or_else(|| Error::input(format!("{field} contains a non-hexadecimal character")))?;
    }
    Ok(out)
}

pub fn hex(bytes: &[u8]) -> String {
    const DIGITS: &[u8] = b"0123456789abcdef";
    let mut s = String::with_capacity(bytes.len() * 2);
    for &b in bytes {
        s.push(DIGITS[(b >> 4) as usize] as char);
        s.push(DIGITS[(b & 15) as usize] as char);
    }
    s
}

pub fn checksum(message: &[u8], polynomial: &[u8]) -> Vec<u8> {
    let degree = polynomial.len() - 1;
    let mut dividend = message.to_vec();
    dividend.resize(message.len() + degree, 0);
    for i in 0..message.len() {
        if dividend[i] == 1 {
            for (j, &coefficient) in polynomial.iter().enumerate() {
                dividend[i + j] ^= coefficient;
            }
        }
    }
    dividend[message.len()..].to_vec()
}

pub fn append_checksum(message: &[u8], polynomial: &[u8]) -> Vec<u8> {
    let mut out = message.to_vec();
    out.extend(checksum(message, polynomial));
    out
}

#[derive(Clone, Copy)]
pub struct Field {
    pub width: usize,
    pub primitive: u8,
    pub first_root: usize,
}

impl Field {
    pub fn multiply(self, mut a: u8, mut b: u8) -> u8 {
        let mut product = 0;
        while b != 0 {
            if b & 1 != 0 {
                product ^= a;
            }
            b >>= 1;
            a <<= 1;
            if a & (1 << self.width) != 0 {
                a ^= self.primitive;
            }
        }
        product
    }

    fn alpha(self, exponent: usize) -> u8 {
        (0..exponent).fold(1, |x, _| self.multiply(x, 2))
    }

    pub fn encode(self, data: &[u8]) -> Vec<u8> {
        let a = self.alpha(self.first_root);
        let b = self.alpha(self.first_root + 1);
        let generator = [1, a ^ b, self.multiply(a, b)];
        let mut division = data.to_vec();
        division.extend([0, 0]);
        for i in 0..data.len() {
            let lead = division[i];
            for j in 0..3 {
                division[i + j] ^= self.multiply(lead, generator[j]);
            }
        }
        let mut word = data.to_vec();
        word.extend_from_slice(&division[data.len()..]);
        word
    }

    pub fn syndromes(self, word: &[u8]) -> [u8; 2] {
        [self.first_root, self.first_root + 1].map(|exponent| {
            let x = self.alpha(exponent);
            word.iter()
                .fold(0, |acc, &coefficient| self.multiply(acc, x) ^ coefficient)
        })
    }
}

pub const GAP_FIELD: Field = Field {
    width: 5,
    primitive: 0x25,
    first_root: 1,
};
pub const ARC_FIELD: Field = Field {
    width: 6,
    primitive: 0x43,
    first_root: 1,
};
pub const META_FIELD: Field = Field {
    width: 4,
    primitive: 0x13,
    first_root: 0,
};
pub const GAP_POLY: &[u8] = &[1, 0, 1, 1, 0, 0];
pub const ARC_POLY: &[u8] = &[1, 0, 1, 1, 0, 1, 1];

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn checksum_preserves_explicit_trailing_zero_degree() {
        let message = [1, 0, 1, 1, 1, 0, 1, 0];
        let codeword = append_checksum(&message, GAP_POLY);
        assert_eq!(codeword.len(), 13);
        assert_eq!(&codeword[11..], &[0, 0]);
        assert!(checksum(&codeword, GAP_POLY).iter().all(|&b| b == 0));
    }
    #[test]
    fn rs_all_fields_have_zero_syndromes_and_detect_one_error() {
        for (field, len) in [(GAP_FIELD, 19), (ARC_FIELD, 9), (META_FIELD, 2)] {
            for seed in 0..64 {
                let data: Vec<_> = (0..len)
                    .map(|i| ((i * 13 + seed) % (1 << field.width)) as u8)
                    .collect();
                let mut word = field.encode(&data);
                assert_eq!(field.syndromes(&word), [0, 0]);
                word[len / 2] ^= 1;
                assert_ne!(field.syndromes(&word), [0, 0]);
            }
        }
    }
}
