use md5::{Digest as _, Md5};
use sha1::Sha1;
use sha2::{Sha256, Sha512};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum HashAlgo {
    Md5,
    Sha1,
    Sha256,
    Sha512,
}

pub const HASH_ALGOS: [HashAlgo; 4] = [
    HashAlgo::Md5,
    HashAlgo::Sha1,
    HashAlgo::Sha256,
    HashAlgo::Sha512,
];

impl HashAlgo {
    pub fn from_index(index: usize) -> Self {
        HASH_ALGOS.get(index).copied().unwrap_or(HashAlgo::Md5)
    }

    pub fn label(self) -> &'static str {
        match self {
            HashAlgo::Md5 => "MD5",
            HashAlgo::Sha1 => "SHA-1",
            HashAlgo::Sha256 => "SHA-256",
            HashAlgo::Sha512 => "SHA-512",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            HashAlgo::Md5 => "MD5：128 位，小写十六进制，不可逆",
            HashAlgo::Sha1 => "SHA-1：160 位，小写十六进制，不可逆",
            HashAlgo::Sha256 => "SHA-256：256 位，小写十六进制，不可逆",
            HashAlgo::Sha512 => "SHA-512：512 位，小写十六进制，不可逆",
        }
    }
}

/// Hash UTF-8 bytes of `input` without trimming. Output is lowercase hex.
pub fn digest_hex(algo: HashAlgo, input: &str) -> String {
    let bytes = input.as_bytes();
    match algo {
        HashAlgo::Md5 => hex(&Md5::digest(bytes)),
        HashAlgo::Sha1 => hex(&Sha1::digest(bytes)),
        HashAlgo::Sha256 => hex(&Sha256::digest(bytes)),
        HashAlgo::Sha512 => hex(&Sha512::digest(bytes)),
    }
}

fn hex(bytes: &[u8]) -> String {
    const LUT: &[u8; 16] = b"0123456789abcdef";
    let mut out = String::with_capacity(bytes.len() * 2);
    for b in bytes {
        out.push(LUT[(b >> 4) as usize] as char);
        out.push(LUT[(b & 0x0f) as usize] as char);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn standard_vectors_empty_a_abc() {
        let cases = [
            (HashAlgo::Md5, "", "d41d8cd98f00b204e9800998ecf8427e"),
            (HashAlgo::Md5, "a", "0cc175b9c0f1b6a831c399e269772661"),
            (HashAlgo::Md5, "abc", "900150983cd24fb0d6963f7d28e17f72"),
            (HashAlgo::Sha1, "", "da39a3ee5e6b4b0d3255bfef95601890afd80709"),
            (HashAlgo::Sha1, "a", "86f7e437faa5a7fce15d1ddcb9eaeaea377667b8"),
            (HashAlgo::Sha1, "abc", "a9993e364706816aba3e25717850c26c9cd0d89d"),
            (
                HashAlgo::Sha256,
                "",
                "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
            ),
            (
                HashAlgo::Sha256,
                "a",
                "ca978112ca1bbdcafac231b39a23dc4da786eff8147c4e72b9807785afee48bb",
            ),
            (
                HashAlgo::Sha256,
                "abc",
                "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
            ),
            (
                HashAlgo::Sha512,
                "",
                "cf83e1357eefb8bdf1542850d66d8007d620e4050b5715dc83f4a921d36ce9ce47d0d13c5d85f2b0ff8318d2877eec2f63b931bd47417a81a538327af927da3e",
            ),
            (
                HashAlgo::Sha512,
                "a",
                "1f40fc92da241694750979ee6cf582f2d5d7d28e18335de05abc54d0560e0f5302860c652bf08d560252aa5e74210546f369fbbbce8c12cfc7957b2652fe9a75",
            ),
            (
                HashAlgo::Sha512,
                "abc",
                "ddaf35a193617abacc417349ae20413112e6fa4e89a97ea20a9eeee64b55d39a2192992a274fc1a836ba3c23a3feebbd454d4423643ce80e2a9ac94fa54ca49f",
            ),
        ];
        for (algo, input, expect) in cases {
            assert_eq!(digest_hex(algo, input), expect, "{algo:?} {input:?}");
        }
    }

    #[test]
    fn does_not_trim_whitespace() {
        let spaced = digest_hex(HashAlgo::Md5, " abc");
        let plain = digest_hex(HashAlgo::Md5, "abc");
        assert_ne!(spaced, plain);
        assert_eq!(spaced, "12cfaf7fd98f33be8038b3d56c18f061");
    }
}
