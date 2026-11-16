//! RFC 1320 MD4 checksum for EDuke32 map-hack version matching.

/// Compute an MD4 digest.
///
/// # Errors
///
/// Returns an error only if the padded message length cannot be represented.
pub fn digest(data: &[u8]) -> Result<[u8; 16], crate::BuildError> {
    let prefix_len = data
        .len()
        .checked_add(1)
        .ok_or(crate::BuildError::Invalid("MD4 input size"))?;
    let zero_padding = (56 + 64 - prefix_len % 64) % 64;
    let padded_len = prefix_len
        .checked_add(zero_padding)
        .and_then(|len| len.checked_add(8))
        .ok_or(crate::BuildError::Invalid("MD4 input size"))?;
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut message = Vec::with_capacity(padded_len);
    message.extend_from_slice(data);
    message.push(0x80);
    message.resize(padded_len - 8, 0);
    message.extend_from_slice(&bit_len.to_le_bytes());

    let mut state = [0x6745_2301u32, 0xefcd_ab89, 0x98ba_dcfe, 0x1032_5476];
    for chunk in message.chunks_exact(64) {
        let mut words = [0u32; 16];
        for (word, bytes) in words.iter_mut().zip(chunk.chunks_exact(4)) {
            *word = u32::from_le_bytes([bytes[0], bytes[1], bytes[2], bytes[3]]);
        }
        let [a0, b0, c0, d0] = state;
        let (mut a, mut b, mut c, mut d) = (a0, b0, c0, d0);

        for i in 0..16 {
            let f = (b & c) | (!b & d);
            let (word, shift) = (i, [3, 7, 11, 19][i % 4]);
            (a, b, c, d) = (
                d,
                a.wrapping_add(f)
                    .wrapping_add(words[word])
                    .rotate_left(shift),
                b,
                c,
            );
        }
        for i in 0..16 {
            let g = (b & c) | (b & d) | (c & d);
            let word = (i % 4) * 4 + i / 4;
            let shift = [3, 5, 9, 13][i % 4];
            (a, b, c, d) = (
                d,
                a.wrapping_add(g)
                    .wrapping_add(words[word])
                    .wrapping_add(0x5a82_7999)
                    .rotate_left(shift),
                b,
                c,
            );
        }
        for i in 0..16 {
            const ORDER: [usize; 16] = [0, 8, 4, 12, 2, 10, 6, 14, 1, 9, 5, 13, 3, 11, 7, 15];
            let h = b ^ c ^ d;
            let shift = [3, 9, 11, 15][i % 4];
            (a, b, c, d) = (
                d,
                a.wrapping_add(h)
                    .wrapping_add(words[ORDER[i]])
                    .wrapping_add(0x6ed9_eba1)
                    .rotate_left(shift),
                b,
                c,
            );
        }
        state[0] = a0.wrapping_add(a);
        state[1] = b0.wrapping_add(b);
        state[2] = c0.wrapping_add(c);
        state[3] = d0.wrapping_add(d);
    }

    let mut digest = [0u8; 16];
    for (bytes, word) in digest.chunks_exact_mut(4).zip(state) {
        bytes.copy_from_slice(&word.to_le_bytes());
    }
    Ok(digest)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(bytes: &[u8]) -> String {
        bytes.iter().map(|byte| format!("{byte:02x}")).collect()
    }

    #[test]
    fn matches_rfc_1320_vectors() {
        for (message, expected) in [
            (b"".as_slice(), "31d6cfe0d16ae931b73c59d7e0c089c0"),
            (b"a".as_slice(), "bde52cb31de33e46245e05fbdbd6fb24"),
            (b"abc".as_slice(), "a448017aaf21d8525fc10ae87aa6729d"),
            (
                b"message digest".as_slice(),
                "d9130a8164549fe818874806e1c7014b",
            ),
        ] {
            assert_eq!(hex(&digest(message).unwrap()), expected);
        }
    }
}
