use std::fmt::Write;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CodecKind {
    Unicode,
    Utf8,
    Url,
    Hex,
    Base64,
    Case,
}

pub const CODEC_KINDS: [CodecKind; 6] = [
    CodecKind::Unicode,
    CodecKind::Utf8,
    CodecKind::Url,
    CodecKind::Hex,
    CodecKind::Base64,
    CodecKind::Case,
];

impl CodecKind {
    pub fn from_index(index: usize) -> Self {
        CODEC_KINDS.get(index).copied().unwrap_or(CodecKind::Unicode)
    }

    pub fn label(self) -> &'static str {
        match self {
            CodecKind::Unicode => "Unicode",
            CodecKind::Utf8 => "UTF-8",
            CodecKind::Url => "URL",
            CodecKind::Hex => "Hex",
            CodecKind::Base64 => "Base64",
            CodecKind::Case => "大小写",
        }
    }

    pub fn hint(self) -> &'static str {
        match self {
            CodecKind::Unicode => "Unicode：文本 ↔ \\uXXXX",
            CodecKind::Utf8 => "UTF-8：文本 ↔ \\xHH 字节",
            CodecKind::Url => "URL：文本 ↔ %HH 百分号编码",
            CodecKind::Hex => "Hex：文本 ↔ UTF-8 十六进制",
            CodecKind::Base64 => "Base64：文本 ↔ Base64",
            CodecKind::Case => "大小写：编码→大写，解码→小写",
        }
    }

    pub fn encode_label(self) -> &'static str {
        match self {
            CodecKind::Case => "大写",
            _ => "编码",
        }
    }

    pub fn decode_label(self) -> &'static str {
        match self {
            CodecKind::Case => "小写",
            _ => "解码",
        }
    }
}

pub fn empty_input(text: &str) -> bool {
    text.is_empty()
}

pub fn encode(kind: CodecKind, input: &str) -> Result<String, String> {
    match kind {
        CodecKind::Unicode => Ok(encode_unicode(input)),
        CodecKind::Utf8 => Ok(encode_utf8(input)),
        CodecKind::Url => Ok(encode_url(input)),
        CodecKind::Hex => Ok(encode_hex(input)),
        CodecKind::Base64 => Ok(encode_base64(input.as_bytes())),
        CodecKind::Case => Ok(input.to_uppercase()),
    }
}

pub fn decode(kind: CodecKind, input: &str) -> Result<String, String> {
    match kind {
        CodecKind::Unicode => decode_unicode(input),
        CodecKind::Utf8 => decode_utf8(input),
        CodecKind::Url => decode_url(input),
        CodecKind::Hex => decode_hex(input),
        CodecKind::Base64 => decode_base64_to_text(input),
        CodecKind::Case => Ok(input.to_lowercase()),
    }
}

fn encode_unicode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 6);
    for ch in input.chars() {
        let n = ch as u32;
        if n <= 0xFFFF {
            let _ = write!(out, "\\u{n:04x}");
        } else {
            let _ = write!(out, "\\u{{{n:x}}}");
        }
    }
    out
}

fn decode_unicode(input: &str) -> Result<String, String> {
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut out = String::with_capacity(input.len());
    while i < chars.len() {
        if chars[i] != '\\' || i + 1 >= chars.len() {
            out.push(chars[i]);
            i += 1;
            continue;
        }
        match chars[i + 1] {
            'u' => {
                if i + 2 < chars.len() && chars[i + 2] == '{' {
                    let close = chars[i + 3..]
                        .iter()
                        .position(|&c| c == '}')
                        .ok_or_else(|| "不完整的 \\u{…} 转义".to_string())?;
                    let hex = &chars[i + 3..i + 3 + close];
                    if hex.is_empty() || hex.len() > 6 || !hex.iter().all(|c| c.is_ascii_hexdigit()) {
                        return Err("非法的 \\u{…} 转义".into());
                    }
                    let n = parse_hex_chars(hex)?;
                    let ch = char::from_u32(n)
                        .ok_or_else(|| format!("非法 Unicode 码点: U+{n:X}"))?;
                    out.push(ch);
                    i += 4 + close;
                } else {
                    let n = parse_hex_slice(&chars, i + 2, 4)?;
                    i += 6;
                    if (0xD800..=0xDBFF).contains(&n) {
                        if i + 6 <= chars.len()
                            && chars[i] == '\\'
                            && chars[i + 1] == 'u'
                            && chars[i + 2] != '{'
                        {
                            let low = parse_hex_slice(&chars, i + 2, 4)?;
                            if (0xDC00..=0xDFFF).contains(&low) {
                                let cp = 0x10000 + ((n - 0xD800) << 10) + (low - 0xDC00);
                                let ch = char::from_u32(cp)
                                    .ok_or_else(|| format!("非法代理对: U+{n:X} U+{low:X}"))?;
                                out.push(ch);
                                i += 6;
                                continue;
                            }
                        }
                        return Err("孤立的 UTF-16 高代理项".into());
                    }
                    if (0xDC00..=0xDFFF).contains(&n) {
                        return Err("孤立的 UTF-16 低代理项".into());
                    }
                    let ch = char::from_u32(n)
                        .ok_or_else(|| format!("非法 Unicode 码点: U+{n:X}"))?;
                    out.push(ch);
                }
            }
            'U' => {
                let n = parse_hex_slice(&chars, i + 2, 8)?;
                let ch = char::from_u32(n)
                    .ok_or_else(|| format!("非法 Unicode 码点: U+{n:X}"))?;
                out.push(ch);
                i += 10;
            }
            '\\' => {
                out.push('\\');
                i += 2;
            }
            _ => {
                out.push('\\');
                i += 1;
            }
        }
    }
    Ok(out)
}

fn encode_utf8(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 4);
    for b in input.as_bytes() {
        let _ = write!(out, "\\x{b:02x}");
    }
    out
}

fn decode_utf8(input: &str) -> Result<String, String> {
    if !input.contains("\\x") && !input.contains("\\X") {
        return decode_hex(input);
    }
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    let mut bytes = Vec::new();
    while i < chars.len() {
        if chars[i].is_whitespace() {
            i += 1;
            continue;
        }
        if chars[i] == '\\' && i + 1 < chars.len() && (chars[i + 1] == 'x' || chars[i + 1] == 'X') {
            if i + 2 < chars.len() && chars[i + 2] == '{' {
                let close = chars[i + 3..]
                    .iter()
                    .position(|&c| c == '}')
                    .ok_or_else(|| "不完整的 \\x{…} 转义".to_string())?;
                let hex = &chars[i + 3..i + 3 + close];
                if hex.is_empty() || hex.len() > 2 || !hex.iter().all(|c| c.is_ascii_hexdigit()) {
                    return Err("非法的 \\x{…} 转义".into());
                }
                bytes.push(parse_hex_chars(hex)? as u8);
                i += 4 + close;
            } else {
                let n = parse_hex_slice(&chars, i + 2, 2)?;
                bytes.push(n as u8);
                i += 4;
            }
        } else {
            return Err(format!("无法解析 UTF-8 转义: {}", chars[i]));
        }
    }
    bytes_to_utf8(bytes, "UTF-8")
}

fn encode_url(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.as_bytes() {
        match b {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            _ => {
                let _ = write!(out, "%{b:02X}");
            }
        }
    }
    out
}

fn decode_url(input: &str) -> Result<String, String> {
    let bytes = input.as_bytes();
    let mut out = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        match bytes[i] {
            b'%' => {
                if i + 2 >= bytes.len() {
                    return Err("不完整的 %HH 序列".into());
                }
                let hi = bytes[i + 1] as char;
                let lo = bytes[i + 2] as char;
                if !hi.is_ascii_hexdigit() || !lo.is_ascii_hexdigit() {
                    return Err(format!("非法百分号编码: %{hi}{lo}"));
                }
                out.push(parse_hex_pair(hi, lo)?);
                i += 3;
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    bytes_to_utf8(out, "URL")
}

fn encode_hex(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for (i, b) in input.as_bytes().iter().enumerate() {
        if i > 0 {
            out.push(' ');
        }
        let _ = write!(out, "{b:02X}");
    }
    out
}

fn decode_hex(input: &str) -> Result<String, String> {
    let mut hex = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c.is_ascii_whitespace() || c == ':' || c == '-' || c == ',' {
            i += 1;
            continue;
        }
        if c == '0'
            && i + 1 < chars.len()
            && (chars[i + 1] == 'x' || chars[i + 1] == 'X')
        {
            i += 2;
            continue;
        }
        if c.is_ascii_hexdigit() {
            hex.push(c);
            i += 1;
        } else {
            return Err(format!("非法十六进制字符: {c}"));
        }
    }
    if hex.is_empty() {
        return Ok(String::new());
    }
    if hex.len() % 2 != 0 {
        return Err("十六进制长度必须为偶数".into());
    }
    let mut bytes = Vec::with_capacity(hex.len() / 2);
    let raw = hex.as_bytes();
    for chunk in raw.chunks(2) {
        let hi = chunk[0] as char;
        let lo = chunk[1] as char;
        bytes.push(parse_hex_pair(hi, lo)?);
    }
    bytes_to_utf8(bytes, "Hex")
}

const B64_TABLE: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";

fn encode_base64(data: &[u8]) -> String {
    let mut out = String::with_capacity(data.len().div_ceil(3) * 4);
    for chunk in data.chunks(3) {
        let a = chunk[0];
        let b = chunk.get(1).copied().unwrap_or(0);
        let c = chunk.get(2).copied().unwrap_or(0);
        let n = u32::from(a) << 16 | u32::from(b) << 8 | u32::from(c);
        out.push(B64_TABLE[((n >> 18) & 63) as usize] as char);
        out.push(B64_TABLE[((n >> 12) & 63) as usize] as char);
        if chunk.len() > 1 {
            out.push(B64_TABLE[((n >> 6) & 63) as usize] as char);
        } else {
            out.push('=');
        }
        if chunk.len() > 2 {
            out.push(B64_TABLE[(n & 63) as usize] as char);
        } else {
            out.push('=');
        }
    }
    out
}

fn decode_base64_to_text(input: &str) -> Result<String, String> {
    bytes_to_utf8(decode_base64(input)?, "Base64")
}

fn decode_base64(input: &str) -> Result<Vec<u8>, String> {
    let mut filtered = Vec::with_capacity(input.len());
    for b in input.bytes() {
        if b.is_ascii_whitespace() {
            continue;
        }
        filtered.push(b);
    }
    if filtered.is_empty() {
        return Ok(Vec::new());
    }
    let pad = filtered.iter().rev().take_while(|&&b| b == b'=').count();
    if pad > 2 {
        return Err("非法的 Base64 填充".into());
    }
    let data_len = filtered.len() - pad;
    if !filtered[..data_len].iter().all(|&b| b64_val(b).is_some()) {
        return Err("非法的 Base64 字符".into());
    }
    if filtered[data_len..].iter().any(|&b| b != b'=') {
        return Err("非法的 Base64 填充".into());
    }
    let mut padded = filtered[..data_len].to_vec();
    while padded.len() % 4 != 0 {
        padded.push(b'A');
    }
    let extra = (4 - (data_len % 4)) % 4;
    let mut out = Vec::with_capacity(padded.len() / 4 * 3);
    for chunk in padded.chunks(4) {
        let n = (u32::from(b64_val(chunk[0]).unwrap()) << 18)
            | (u32::from(b64_val(chunk[1]).unwrap()) << 12)
            | (u32::from(b64_val(chunk[2]).unwrap()) << 6)
            | u32::from(b64_val(chunk[3]).unwrap());
        out.push(((n >> 16) & 0xFF) as u8);
        out.push(((n >> 8) & 0xFF) as u8);
        out.push((n & 0xFF) as u8);
    }
    let drop = if extra == 0 { pad } else { extra };
    out.truncate(out.len().saturating_sub(drop));
    Ok(out)
}

fn b64_val(c: u8) -> Option<u8> {
    match c {
        b'A'..=b'Z' => Some(c - b'A'),
        b'a'..=b'z' => Some(c - b'a' + 26),
        b'0'..=b'9' => Some(c - b'0' + 52),
        b'+' | b'-' => Some(62),
        b'/' | b'_' => Some(63),
        _ => None,
    }
}

fn parse_hex_slice(chars: &[char], start: usize, len: usize) -> Result<u32, String> {
    if start + len > chars.len() {
        return Err("不完整的十六进制转义".into());
    }
    parse_hex_chars(&chars[start..start + len])
}

fn parse_hex_chars(chars: &[char]) -> Result<u32, String> {
    let mut n = 0u32;
    for &c in chars {
        let d = c
            .to_digit(16)
            .ok_or_else(|| format!("非法十六进制字符: {c}"))?;
        n = n
            .checked_mul(16)
            .and_then(|v| v.checked_add(d))
            .ok_or_else(|| "十六进制数值过大".to_string())?;
    }
    Ok(n)
}

fn parse_hex_pair(hi: char, lo: char) -> Result<u8, String> {
    let n = parse_hex_chars(&[hi, lo])?;
    u8::try_from(n).map_err(|_| "十六进制字节超出范围".to_string())
}

fn bytes_to_utf8(bytes: Vec<u8>, kind: &str) -> Result<String, String> {
    String::from_utf8(bytes).map_err(|_| format!("{kind} 解码结果不是合法 UTF-8 文本"))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn roundtrip(kind: CodecKind, text: &str) {
        let encoded = encode(kind, text).unwrap();
        let decoded = decode(kind, &encoded).unwrap();
        assert_eq!(decoded, text, "{kind:?} roundtrip failed: {encoded}");
    }

    #[test]
    fn roundtrips_all_codecs() {
        for kind in [
            CodecKind::Unicode,
            CodecKind::Utf8,
            CodecKind::Url,
            CodecKind::Hex,
            CodecKind::Base64,
        ] {
            roundtrip(kind, "hello");
            roundtrip(kind, "你好");
            roundtrip(kind, "😀");
            roundtrip(kind, "A + B = C\n\t~_-.");
        }
    }

    #[test]
    fn unicode_uses_js_escapes() {
        assert_eq!(encode_unicode("你好"), r"\u4f60\u597d");
        assert_eq!(decode_unicode(r"\u4F60\u597D").unwrap(), "你好");
        assert_eq!(encode_unicode("😀"), r"\u{1f600}");
        assert_eq!(decode_unicode(r"\ud83d\ude00").unwrap(), "😀");
        assert_eq!(decode_unicode(r"\U0001F600").unwrap(), "😀");
        assert_eq!(decode_unicode(r"hello\u4e2d").unwrap(), "hello中");
    }

    #[test]
    fn utf8_uses_byte_escapes() {
        assert_eq!(encode_utf8("你"), r"\xe4\xbd\xa0");
        assert_eq!(decode_utf8(r"\xE4\xBD\xA0").unwrap(), "你");
        assert_eq!(decode_utf8(r"\x{e4}\x{bd}\x{a0}").unwrap(), "你");
        assert_eq!(decode_utf8("E4 BD A0").unwrap(), "你");
    }

    #[test]
    fn url_percent_encodes_non_unreserved() {
        assert_eq!(encode_url("hello world"), "hello%20world");
        assert_eq!(encode_url("你好"), "%E4%BD%A0%E5%A5%BD");
        assert_eq!(decode_url("hello+world").unwrap(), "hello world");
        assert_eq!(decode_url("%E4%BD%A0").unwrap(), "你");
        assert_eq!(encode_url("A-Z_a.z~"), "A-Z_a.z~");
    }

    #[test]
    fn hex_spaces_and_prefixes() {
        assert_eq!(encode_hex("Hi"), "48 69");
        assert_eq!(decode_hex("4869").unwrap(), "Hi");
        assert_eq!(decode_hex("0x48 0x69").unwrap(), "Hi");
        assert_eq!(decode_hex("48:69").unwrap(), "Hi");
    }

    #[test]
    fn base64_padding_and_url_safe() {
        assert_eq!(encode_base64(b"hello"), "aGVsbG8=");
        assert_eq!(decode_base64_to_text("aGVsbG8").unwrap(), "hello");
        assert_eq!(decode_base64_to_text("aGVsbG8=").unwrap(), "hello");
        let encoded = encode_base64("你好".as_bytes());
        assert_eq!(decode_base64_to_text(&encoded).unwrap(), "你好");
        let url_safe = encoded
            .replace('+', "-")
            .replace('/', "_")
            .trim_end_matches('=')
            .to_string();
        assert_eq!(decode_base64_to_text(&url_safe).unwrap(), "你好");
    }

    #[test]
    fn case_upper_and_lower() {
        assert_eq!(encode(CodecKind::Case, "Hello 世界").unwrap(), "HELLO 世界");
        assert_eq!(decode(CodecKind::Case, "Hello 世界").unwrap(), "hello 世界");
        assert_eq!(encode(CodecKind::Case, "straße").unwrap(), "STRASSE");
        assert_eq!(decode(CodecKind::Case, "İ").unwrap(), "i\u{307}");
    }

    #[test]
    fn rejects_invalid_input() {
        assert!(decode_unicode(r"\u12").is_err());
        assert!(decode_unicode(r"\ud83d").is_err());
        assert!(decode_url("%E4%BD").is_err());
        assert!(decode_hex("GGG").is_err());
        assert!(decode_hex("ABC").is_err());
        assert!(decode_base64_to_text("!!!!").is_err());
        assert!(decode_utf8(r"\xzz").is_err());
    }

    #[test]
    fn empty_is_identity() {
        for kind in CODEC_KINDS {
            assert_eq!(encode(kind, "").unwrap(), "");
            assert_eq!(decode(kind, "").unwrap(), "");
        }
    }
}
