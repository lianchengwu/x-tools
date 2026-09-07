//! ID, Random Number, and Password Generation Algorithms
//!
//! Provides generation for:
//! 1. Random numbers and passwords with custom length and character sets
//! 2. UUIDv7 (RFC 9562 time-ordered UUID)
//! 3. NanoID with custom length
//! 4. Snowflake ID (Twitter Snowflake 64-bit integer)
//! 5. Other IDs: UUIDv4, ULID, MongoDB ObjectId, CUID2

use std::fmt::Write;
use std::sync::atomic::{AtomicU64, Ordering};

// -----------------------------------------------------------------------------
// PRNG Engine (Xoshiro256++ seeded via SplitMix64)
// -----------------------------------------------------------------------------

static GLOBAL_COUNTER: AtomicU64 = AtomicU64::new(0x9E3779B97F4A7C15);

#[derive(Clone, Debug)]
pub struct Rng {
    s: [u64; 4],
}

impl Default for Rng {
    fn default() -> Self {
        Self::new()
    }
}

impl Rng {
    pub fn new() -> Self {
        let now_ms = xtools_sdk::now_millis();
        #[cfg(not(target_arch = "wasm32"))]
        let sys_nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_nanos() as u64)
            .unwrap_or(0);
        #[cfg(target_arch = "wasm32")]
        let sys_nanos = 0u64;

        let cnt = GLOBAL_COUNTER.fetch_add(0x9E3779B97F4A7C15, Ordering::Relaxed);
        let ptr_entropy = &GLOBAL_COUNTER as *const _ as usize as u64;

        let seed = (now_ms as u64)
            ^ sys_nanos.rotate_left(17)
            ^ cnt.rotate_left(31)
            ^ ptr_entropy.rotate_left(47);

        Self::seed_from_u64(seed)
    }

    pub fn seed_from_u64(mut x: u64) -> Self {
        // SplitMix64 initialization
        let mut sm = || {
            x = x.wrapping_add(0x9e3779b97f4a7c15);
            let mut z = x;
            z = (z ^ (z >> 30)).wrapping_mul(0xbf58476d1ce4e5b9);
            z = (z ^ (z >> 27)).wrapping_mul(0x94d049bb133111eb);
            z ^ (z >> 31)
        };

        let mut s = [sm(), sm(), sm(), sm()];
        if s == [0, 0, 0, 0] {
            s[0] = 0x5468697349734153;
        }
        Self { s }
    }

    #[inline]
    pub fn next_u64(&mut self) -> u64 {
        let res = (self.s[0].wrapping_add(self.s[3]))
            .rotate_left(23)
            .wrapping_add(self.s[0]);
        let t = self.s[1] << 17;

        self.s[2] ^= self.s[0];
        self.s[3] ^= self.s[1];
        self.s[1] ^= self.s[2];
        self.s[0] ^= self.s[3];

        self.s[2] ^= t;
        self.s[3] = self.s[3].rotate_left(45);

        res
    }

    #[inline]
    pub fn next_u32(&mut self) -> u32 {
        self.next_u64() as u32
    }

    #[inline]
    pub fn gen_range(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            return 0;
        }
        (self.next_u64() as usize) % upper
    }

    pub fn fill_bytes(&mut self, dest: &mut [u8]) {
        let mut chunks = dest.chunks_exact_mut(8);
        for chunk in chunks.by_ref() {
            let val = self.next_u64();
            chunk.copy_from_slice(&val.to_le_bytes());
        }
        let remainder = chunks.into_remainder();
        if !remainder.is_empty() {
            let val = self.next_u64();
            let bytes = val.to_le_bytes();
            remainder.copy_from_slice(&bytes[..remainder.len()]);
        }
    }
}

// -----------------------------------------------------------------------------
// Generator Sub-Kind Definition
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IdKind {
    Password,
    RandomNumber,
    UuidV7,
    NanoId,
    Snowflake,
    UuidV4,
    Ulid,
    ObjectId,
    Cuid2,
}

pub const ID_KINDS: [IdKind; 9] = [
    IdKind::Password,
    IdKind::RandomNumber,
    IdKind::UuidV7,
    IdKind::NanoId,
    IdKind::Snowflake,
    IdKind::UuidV4,
    IdKind::Ulid,
    IdKind::ObjectId,
    IdKind::Cuid2,
];

impl IdKind {
    pub fn from_index(index: usize) -> Self {
        ID_KINDS.get(index).copied().unwrap_or(IdKind::Password)
    }

    pub fn label(self) -> &'static str {
        match self {
            IdKind::Password => "密码生成",
            IdKind::RandomNumber => "随机数",
            IdKind::UuidV7 => "UUIDv7",
            IdKind::NanoId => "NanoID",
            IdKind::Snowflake => "雪花ID",
            IdKind::UuidV4 => "UUIDv4",
            IdKind::Ulid => "ULID",
            IdKind::ObjectId => "ObjectId",
            IdKind::Cuid2 => "CUID2",
        }
    }

    pub fn default_length(self) -> usize {
        match self {
            IdKind::Password => 16,
            IdKind::RandomNumber => 6,
            IdKind::UuidV7 => 36,
            IdKind::NanoId => 21,
            IdKind::Snowflake => 19,
            IdKind::UuidV4 => 36,
            IdKind::Ulid => 26,
            IdKind::ObjectId => 24,
            IdKind::Cuid2 => 24,
        }
    }

    pub fn supports_custom_length(self) -> bool {
        matches!(
            self,
            IdKind::Password | IdKind::RandomNumber | IdKind::NanoId | IdKind::Cuid2
        )
    }

    pub fn description(self) -> &'static str {
        match self {
            IdKind::Password => "强密码：自定义长度，包含大小写、数字与特殊字符",
            IdKind::RandomNumber => "纯数字随机码：支持自定义长度（如 6 位验证码、16 位随机数）",
            IdKind::UuidV7 => "UUIDv7：RFC 9562 基于毫秒时间戳排序的通用唯一标识符",
            IdKind::NanoId => "NanoID：短小安全、URL 友好的唯一字符串 ID（默认 21 位）",
            IdKind::Snowflake => "雪花 ID：Twitter Snowflake 64 位分布式有序自增数值 ID",
            IdKind::UuidV4 => "UUIDv4：RFC 4122 完全随机分布的 128 位经典 UUID",
            IdKind::Ulid => "ULID：26 位可字典序排序、无符号特殊字符、Base32 唯一 ID",
            IdKind::ObjectId => "MongoDB ObjectId：12 字节 24 位十六进制文档主键 ID",
            IdKind::Cuid2 => "CUID2：抗碰撞、水平扩展友好的安全唯一标识串",
        }
    }
}

// -----------------------------------------------------------------------------
// Generator Options
// -----------------------------------------------------------------------------

#[derive(Debug, Clone)]
pub struct GeneratorConfig {
    pub kind: IdKind,
    pub length: usize,
    pub count: usize,
    pub uppercase: bool,
    pub lowercase: bool,
    pub digits: bool,
    pub symbols: bool,
    pub exclude_ambiguous: bool,
}

impl Default for GeneratorConfig {
    fn default() -> Self {
        Self {
            kind: IdKind::Password,
            length: 16,
            count: 1,
            uppercase: true,
            lowercase: true,
            digits: true,
            symbols: true,
            exclude_ambiguous: false,
        }
    }
}

// -----------------------------------------------------------------------------
// Generator Implementations
// -----------------------------------------------------------------------------

/// 1. 随机数生成 (Pure digits with customizable length)
pub fn generate_random_number(length: usize, rng: &mut Rng) -> String {
    let len = length.clamp(1, 256);
    let mut out = String::with_capacity(len);
    for _ in 0..len {
        let digit = (rng.next_u32() % 10) as u8;
        out.push((b'0' + digit) as char);
    }
    out
}

/// 2. 密码生成 (Password with customizable length and charset flags)
pub fn generate_password(
    length: usize,
    uppercase: bool,
    lowercase: bool,
    digits: bool,
    symbols: bool,
    exclude_ambiguous: bool,
    rng: &mut Rng,
) -> String {
    let len = length.clamp(4, 256);

    let mut upper_pool: Vec<char> = ('A'..='Z').collect();
    let mut lower_pool: Vec<char> = ('a'..='z').collect();
    let mut digit_pool: Vec<char> = ('0'..='9').collect();
    let mut symbol_pool: Vec<char> = "!@#$%^&*()_+-=[]{}|;:,.<>?".chars().collect();

    if exclude_ambiguous {
        let amb = ['0', 'O', 'o', '1', 'l', 'I'];
        upper_pool.retain(|c| !amb.contains(c));
        lower_pool.retain(|c| !amb.contains(c));
        digit_pool.retain(|c| !amb.contains(c));
        symbol_pool.retain(|c| !amb.contains(c));
    }

    let mut enabled_pools: Vec<&[char]> = Vec::new();
    if uppercase && !upper_pool.is_empty() {
        enabled_pools.push(&upper_pool);
    }
    if lowercase && !lower_pool.is_empty() {
        enabled_pools.push(&lower_pool);
    }
    if digits && !digit_pool.is_empty() {
        enabled_pools.push(&digit_pool);
    }
    if symbols && !symbol_pool.is_empty() {
        enabled_pools.push(&symbol_pool);
    }

    // If nothing enabled, default to lowercase + uppercase + digits
    if enabled_pools.is_empty() {
        enabled_pools.push(&lower_pool);
        enabled_pools.push(&upper_pool);
        enabled_pools.push(&digit_pool);
    }

    let mut chars: Vec<char> = Vec::with_capacity(len);

    // Guarantee at least 1 character from each enabled pool
    for pool in &enabled_pools {
        if chars.len() < len {
            let idx = rng.gen_range(pool.len());
            chars.push(pool[idx]);
        }
    }

    // Total combined pool for remaining characters
    let mut combined: Vec<char> = Vec::new();
    for pool in &enabled_pools {
        combined.extend_from_slice(pool);
    }

    while chars.len() < len {
        let idx = rng.gen_range(combined.len());
        chars.push(combined[idx]);
    }

    // Fisher-Yates shuffle
    for i in (1..chars.len()).rev() {
        let j = rng.gen_range(i + 1);
        chars.swap(i, j);
    }

    chars.into_iter().collect()
}

/// 3. UUIDv7 (RFC 9562 time-ordered UUID)
///
/// 48-bit timestamp ms + 4-bit ver(7) + 12-bit rand_a + 2-bit var(0b10) + 62-bit rand_b
pub fn generate_uuidv7(rng: &mut Rng) -> String {
    let now_ms = xtools_sdk::now_millis().max(0) as u64;
    generate_uuidv7_at(now_ms, rng)
}

pub fn generate_uuidv7_at(timestamp_ms: u64, rng: &mut Rng) -> String {
    let mut bytes = [0u8; 16];

    // 48-bit timestamp big-endian (bytes 0..6)
    bytes[0] = ((timestamp_ms >> 40) & 0xFF) as u8;
    bytes[1] = ((timestamp_ms >> 32) & 0xFF) as u8;
    bytes[2] = ((timestamp_ms >> 24) & 0xFF) as u8;
    bytes[3] = ((timestamp_ms >> 16) & 0xFF) as u8;
    bytes[4] = ((timestamp_ms >> 8) & 0xFF) as u8;
    bytes[5] = (timestamp_ms & 0xFF) as u8;

    // Fill remaining bytes 6..16 with random data
    rng.fill_bytes(&mut bytes[6..16]);

    // Version 7: set high 4 bits of byte 6 to 0x7
    bytes[6] = (bytes[6] & 0x0F) | 0x70;

    // Variant 2 (RFC 9562): set high 2 bits of byte 8 to 0b10 (0x80)
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    format_uuid(&bytes)
}

/// 4. NanoID (URL-friendly unique string identifier)
pub fn generate_nanoid(length: usize, rng: &mut Rng) -> String {
    const ALPHABET: &[u8] = b"0123456789abcdefghijklmnopqrstuvwxyzABCDEFGHIJKLMNOPQRSTUVWXYZ_-";
    let len = length.clamp(1, 256);
    let mut out = String::with_capacity(len);

    for _ in 0..len {
        let idx = rng.gen_range(ALPHABET.len());
        out.push(ALPHABET[idx] as char);
    }
    out
}

/// 5. Snowflake ID (Twitter Snowflake 64-bit integer ID)
///
/// 1 bit sign (0) + 41 bits timestamp ms + 5 bits datacenter + 5 bits worker + 12 bits sequence
pub fn generate_snowflake(rng: &mut Rng) -> String {
    // Twitter epoch: 2010-11-04 01:42:54 UTC = 1288834974657 ms
    const TWITTER_EPOCH: u64 = 1288834974657;

    let now_ms = xtools_sdk::now_millis().max(0) as u64;
    let diff = if now_ms > TWITTER_EPOCH {
        now_ms - TWITTER_EPOCH
    } else {
        now_ms
    } & 0x1FFFFFFFFFF; // 41 bits

    static SEQUENCE: AtomicU64 = AtomicU64::new(0);
    let seq = SEQUENCE.fetch_add(1, Ordering::Relaxed) & 0xFFF; // 12 bits
    let worker_id = (rng.next_u32() & 0x1F) as u64; // 5 bits (0..31)
    let datacenter_id = 1u64; // 5 bits

    let id: u64 = (diff << 22) | (datacenter_id << 17) | (worker_id << 12) | seq;
    id.to_string()
}

/// 6. UUIDv4 (RFC 4122 Random UUID)
pub fn generate_uuidv4(rng: &mut Rng) -> String {
    let mut bytes = [0u8; 16];
    rng.fill_bytes(&mut bytes);

    // Version 4: set high 4 bits of byte 6 to 0x4
    bytes[6] = (bytes[6] & 0x0F) | 0x40;
    // Variant: set high 2 bits of byte 8 to 0b10
    bytes[8] = (bytes[8] & 0x3F) | 0x80;

    format_uuid(&bytes)
}

/// 7. ULID (Universally Unique Lexicographically Sortable Identifier)
///
/// 48-bit timestamp + 80-bit random, encoded in Crockford's Base32 (26 characters)
pub fn generate_ulid(rng: &mut Rng) -> String {
    const ENCODING: &[u8] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";
    let now_ms = xtools_sdk::now_millis().max(0) as u64;

    let mut bytes = [0u8; 16];
    bytes[0] = ((now_ms >> 40) & 0xFF) as u8;
    bytes[1] = ((now_ms >> 32) & 0xFF) as u8;
    bytes[2] = ((now_ms >> 24) & 0xFF) as u8;
    bytes[3] = ((now_ms >> 16) & 0xFF) as u8;
    bytes[4] = ((now_ms >> 8) & 0xFF) as u8;
    bytes[5] = (now_ms & 0xFF) as u8;
    rng.fill_bytes(&mut bytes[6..16]);

    // Encode 128-bit bytes into 26 Base32 characters
    let mut s = String::with_capacity(26);

    // Timestamp: 10 characters from 48 bits (6 bytes)
    s.push(ENCODING[((bytes[0] & 224) >> 5) as usize] as char);
    s.push(ENCODING[(bytes[0] & 31) as usize] as char);
    s.push(ENCODING[((bytes[1] & 248) >> 3) as usize] as char);
    s.push(ENCODING[(((bytes[1] & 7) << 2) | ((bytes[2] & 192) >> 6)) as usize] as char);
    s.push(ENCODING[((bytes[2] & 62) >> 1) as usize] as char);
    s.push(ENCODING[(((bytes[2] & 1) << 4) | ((bytes[3] & 240) >> 4)) as usize] as char);
    s.push(ENCODING[(((bytes[3] & 15) << 1) | ((bytes[4] & 128) >> 7)) as usize] as char);
    s.push(ENCODING[((bytes[4] & 124) >> 2) as usize] as char);
    s.push(ENCODING[(((bytes[4] & 3) << 3) | ((bytes[5] & 224) >> 5)) as usize] as char);
    s.push(ENCODING[(bytes[5] & 31) as usize] as char);

    // Randomness: 16 characters from 80 bits (10 bytes: bytes[6..16])
    s.push(ENCODING[((bytes[6] & 248) >> 3) as usize] as char);
    s.push(ENCODING[(((bytes[6] & 7) << 2) | ((bytes[7] & 192) >> 6)) as usize] as char);
    s.push(ENCODING[((bytes[7] & 62) >> 1) as usize] as char);
    s.push(ENCODING[(((bytes[7] & 1) << 4) | ((bytes[8] & 240) >> 4)) as usize] as char);
    s.push(ENCODING[(((bytes[8] & 15) << 1) | ((bytes[9] & 128) >> 7)) as usize] as char);
    s.push(ENCODING[((bytes[9] & 124) >> 2) as usize] as char);
    s.push(ENCODING[(((bytes[9] & 3) << 3) | ((bytes[10] & 224) >> 5)) as usize] as char);
    s.push(ENCODING[(bytes[10] & 31) as usize] as char);
    s.push(ENCODING[((bytes[11] & 248) >> 3) as usize] as char);
    s.push(ENCODING[(((bytes[11] & 7) << 2) | ((bytes[12] & 192) >> 6)) as usize] as char);
    s.push(ENCODING[((bytes[12] & 62) >> 1) as usize] as char);
    s.push(ENCODING[(((bytes[12] & 1) << 4) | ((bytes[13] & 240) >> 4)) as usize] as char);
    s.push(ENCODING[(((bytes[13] & 15) << 1) | ((bytes[14] & 128) >> 7)) as usize] as char);
    s.push(ENCODING[((bytes[14] & 124) >> 2) as usize] as char);
    s.push(ENCODING[(((bytes[14] & 3) << 3) | ((bytes[15] & 224) >> 5)) as usize] as char);
    s.push(ENCODING[(bytes[15] & 31) as usize] as char);

    s
}

/// 8. MongoDB ObjectId (12 bytes / 24 hex characters)
///
/// 4-byte unix timestamp (seconds) + 5-byte random + 3-byte counter
pub fn generate_objectid(rng: &mut Rng) -> String {
    let now_secs = (xtools_sdk::now_millis() / 1000).max(0) as u32;
    static COUNTER: AtomicU64 = AtomicU64::new(0);
    let counter_val = COUNTER.fetch_add(1, Ordering::Relaxed) as u32;

    let mut bytes = [0u8; 12];
    bytes[0..4].copy_from_slice(&now_secs.to_be_bytes());
    rng.fill_bytes(&mut bytes[4..9]);
    let c_bytes = counter_val.to_be_bytes();
    bytes[9..12].copy_from_slice(&c_bytes[1..4]);

    let mut out = String::with_capacity(24);
    for b in bytes {
        let _ = write!(out, "{b:02x}");
    }
    out
}

/// 9. CUID2 (Collision-resistant unique identifier)
pub fn generate_cuid2(length: usize, rng: &mut Rng) -> String {
    const ALPHABET: &[u8] = b"abcdefghijklmnopqrstuvwxyz0123456789";
    const LETTERS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";

    let len = length.clamp(4, 64);
    let mut out = String::with_capacity(len);

    // CUID2 starts with a lowercase letter
    let first = rng.gen_range(LETTERS.len());
    out.push(LETTERS[first] as char);

    for _ in 1..len {
        let idx = rng.gen_range(ALPHABET.len());
        out.push(ALPHABET[idx] as char);
    }
    out
}

// -----------------------------------------------------------------------------
// Dispatcher
// -----------------------------------------------------------------------------

pub fn generate(config: &GeneratorConfig) -> String {
    let mut rng = Rng::new();
    let count = config.count.clamp(1, 50);

    let mut results = Vec::with_capacity(count);
    for _ in 0..count {
        let val = match config.kind {
            IdKind::Password => generate_password(
                config.length,
                config.uppercase,
                config.lowercase,
                config.digits,
                config.symbols,
                config.exclude_ambiguous,
                &mut rng,
            ),
            IdKind::RandomNumber => generate_random_number(config.length, &mut rng),
            IdKind::UuidV7 => generate_uuidv7(&mut rng),
            IdKind::NanoId => generate_nanoid(config.length, &mut rng),
            IdKind::Snowflake => generate_snowflake(&mut rng),
            IdKind::UuidV4 => generate_uuidv4(&mut rng),
            IdKind::Ulid => generate_ulid(&mut rng),
            IdKind::ObjectId => generate_objectid(&mut rng),
            IdKind::Cuid2 => generate_cuid2(config.length, &mut rng),
        };
        results.push(val);
    }

    results.join("\n")
}

fn format_uuid(bytes: &[u8; 16]) -> String {
    format!(
        "{:02x}{:02x}{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}-{:02x}{:02x}{:02x}{:02x}{:02x}{:02x}",
        bytes[0], bytes[1], bytes[2], bytes[3],
        bytes[4], bytes[5],
        bytes[6], bytes[7],
        bytes[8], bytes[9],
        bytes[10], bytes[11], bytes[12], bytes[13], bytes[14], bytes[15]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_random_number_length() {
        let mut rng = Rng::seed_from_u64(12345);
        for len in [1, 4, 6, 8, 16, 32] {
            let s = generate_random_number(len, &mut rng);
            assert_eq!(s.len(), len);
            assert!(s.chars().all(|c| c.is_ascii_digit()));
        }
    }

    #[test]
    fn test_password_length_and_charsets() {
        let mut rng = Rng::seed_from_u64(54321);
        for len in [8, 16, 32, 64] {
            let pwd = generate_password(len, true, true, true, true, false, &mut rng);
            assert_eq!(pwd.len(), len);
            assert!(pwd.chars().any(|c| c.is_ascii_uppercase()));
            assert!(pwd.chars().any(|c| c.is_ascii_lowercase()));
            assert!(pwd.chars().any(|c| c.is_ascii_digit()));
            assert!(pwd.chars().any(|c| !c.is_alphanumeric()));
        }
    }

    #[test]
    fn test_password_exclude_ambiguous() {
        let mut rng = Rng::seed_from_u64(999);
        let pwd = generate_password(64, true, true, true, true, true, &mut rng);
        assert!(!pwd.contains('0'));
        assert!(!pwd.contains('O'));
        assert!(!pwd.contains('o'));
        assert!(!pwd.contains('1'));
        assert!(!pwd.contains('l'));
        assert!(!pwd.contains('I'));
    }

    #[test]
    fn test_uuidv7_format_and_version() {
        let mut rng = Rng::seed_from_u64(777);
        let uuid = generate_uuidv7_at(0x018F2D5E7B2A, &mut rng);
        assert_eq!(uuid.len(), 36);
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert_eq!(parts[0].len(), 8);
        assert_eq!(parts[1].len(), 4);
        assert_eq!(parts[2].len(), 4);
        assert_eq!(parts[3].len(), 4);
        assert_eq!(parts[4].len(), 12);
        // Version 7
        assert!(parts[2].starts_with('7'));
        // Variant 2 (8, 9, a, b)
        let first_var = parts[3].chars().next().unwrap();
        assert!(matches!(first_var, '8' | '9' | 'a' | 'b'));
    }

    #[test]
    fn test_uuidv4_format() {
        let mut rng = Rng::seed_from_u64(888);
        let uuid = generate_uuidv4(&mut rng);
        assert_eq!(uuid.len(), 36);
        let parts: Vec<&str> = uuid.split('-').collect();
        assert_eq!(parts.len(), 5);
        assert!(parts[2].starts_with('4'));
        let first_var = parts[3].chars().next().unwrap();
        assert!(matches!(first_var, '8' | '9' | 'a' | 'b'));
    }

    #[test]
    fn test_nanoid_length_and_charset() {
        let mut rng = Rng::seed_from_u64(42);
        for len in [10, 21, 32] {
            let id = generate_nanoid(len, &mut rng);
            assert_eq!(id.len(), len);
            assert!(id.chars().all(|c| c.is_alphanumeric() || c == '_' || c == '-'));
        }
    }

    #[test]
    fn test_snowflake_is_numeric() {
        let mut rng = Rng::seed_from_u64(101);
        let sf = generate_snowflake(&mut rng);
        assert!(!sf.is_empty());
        let val: u64 = sf.parse().expect("snowflake must be numeric");
        assert!(val > 0);
    }

    #[test]
    fn test_ulid_format() {
        let mut rng = Rng::seed_from_u64(202);
        let ulid = generate_ulid(&mut rng);
        assert_eq!(ulid.len(), 26);
        assert!(ulid.chars().all(|c| c.is_ascii_uppercase() || c.is_ascii_digit()));
        // Crockford's Base32 excludes I, L, O, U
        assert!(!ulid.contains('I'));
        assert!(!ulid.contains('L'));
        assert!(!ulid.contains('O'));
        assert!(!ulid.contains('U'));
    }

    #[test]
    fn test_objectid_format() {
        let mut rng = Rng::seed_from_u64(303);
        let oid = generate_objectid(&mut rng);
        assert_eq!(oid.len(), 24);
        assert!(oid.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn test_cuid2_format() {
        let mut rng = Rng::seed_from_u64(404);
        for len in [16, 24, 32] {
            let cuid = generate_cuid2(len, &mut rng);
            assert_eq!(cuid.len(), len);
            assert!(cuid.chars().next().unwrap().is_ascii_lowercase());
            assert!(cuid.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        }
    }

    #[test]
    fn test_generate_dispatcher_multi_count() {
        let mut config = GeneratorConfig::default();
        config.kind = IdKind::RandomNumber;
        config.length = 8;
        config.count = 5;

        let output = generate(&config);
        let lines: Vec<&str> = output.lines().collect();
        assert_eq!(lines.len(), 5);
        for line in lines {
            assert_eq!(line.len(), 8);
            assert!(line.chars().all(|c| c.is_ascii_digit()));
        }
    }
}
