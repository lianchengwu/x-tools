use serde::{Deserialize, Serialize};
use xtools_sdk::host;
use xtools_sdk::HttpRequest;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TransConfig {
    #[serde(default)]
    pub engine_index: usize,
    #[serde(default)]
    pub baidu_appid: String,
    #[serde(default)]
    pub baidu_key: String,
}

impl Default for TransConfig {
    fn default() -> Self {
        Self {
            engine_index: 0,
            baidu_appid: String::new(),
            baidu_key: String::new(),
        }
    }
}

pub const SOURCE_LANGS: &[(&str, &str)] = &[
    ("auto", "自动"),
    ("zh-CN", "中文"),
    ("en", "英语"),
    ("ja", "日语"),
    ("ko", "韩语"),
    ("fr", "法语"),
    ("de", "德语"),
    ("es", "西班牙语"),
    ("ru", "俄语"),
];

pub const TARGET_LANGS: &[(&str, &str)] = &[
    ("zh-CN", "中文"),
    ("en", "英语"),
    ("ja", "日语"),
    ("ko", "韩语"),
    ("fr", "法语"),
    ("de", "德语"),
    ("es", "西班牙语"),
    ("ru", "俄语"),
];

pub fn translate(
    text: &str,
    src_idx: usize,
    dst_idx: usize,
    config: &TransConfig,
) -> Result<String, String> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err("请输入要翻译的文本".to_string());
    }

    let src = SOURCE_LANGS.get(src_idx).map(|(c, _)| *c).unwrap_or("auto");
    let dst = TARGET_LANGS.get(dst_idx).map(|(c, _)| *c).unwrap_or("zh-CN");

    if config.engine_index == 1 {
        translate_baidu(trimmed, src, dst, &config.baidu_appid, &config.baidu_key)
    } else {
        translate_mymemory(trimmed, src, dst)
    }
}

pub fn swap_state(
    src: usize,
    dst: usize,
    mut input: String,
    mut output: String,
) -> (usize, usize, String, String) {
    if src == 0 {
        return (src, dst, input, output);
    }
    let src_code = SOURCE_LANGS.get(src).map(|(c, _)| *c).unwrap_or("auto");
    let dst_code = TARGET_LANGS.get(dst).map(|(c, _)| *c).unwrap_or("zh-CN");

    let new_src = SOURCE_LANGS.iter().position(|&(c, _)| c == dst_code).unwrap_or(0);
    let new_dst = TARGET_LANGS.iter().position(|&(c, _)| c == src_code).unwrap_or(0);

    std::mem::swap(&mut input, &mut output);
    (new_src, new_dst, input, output)
}

fn translate_mymemory(text: &str, src: &str, dst: &str) -> Result<String, String> {
    let src_str = if src == "auto" { "Autodetect" } else { src };
    let encoded_q = url_encode(text);
    let langpair = format!("{src_str}|{dst}");
    let url = format!("https://api.mymemory.translated.net/get?q={encoded_q}&langpair={langpair}");

    let req = HttpRequest::get(url);
    let resp = host::http_request(req).map_err(|e| format!("网络请求失败: {e}"))?;

    if !resp.is_success() {
        return Err(format!("MyMemory 接口返回错误码: {}", resp.status));
    }

    let body_text = resp.text().map_err(|e| format!("解析响应失败: {e}"))?;
    let parsed: MemoryResponse = serde_json::from_str(&body_text)
        .map_err(|e| format!("解析 MyMemory JSON 响应失败: {e}"))?;

    parsed.into_text()
}

fn translate_baidu(
    text: &str,
    src: &str,
    dst: &str,
    appid: &str,
    key: &str,
) -> Result<String, String> {
    if appid.trim().is_empty() || key.trim().is_empty() {
        return Err("请先在「引擎设置」中配置百度翻译 AppID 与 密钥 (Key)".to_string());
    }

    let from_lang = to_baidu_lang(src);
    let to_lang = to_baidu_lang(dst);
    let salt = host::now_millis().to_string();
    let sign_input = format!("{}{}{}{}", appid.trim(), text, salt, key.trim());
    let sign = md5_hex(sign_input.as_bytes());

    let body_str = format!(
        "q={}&from={}&to={}&appid={}&salt={}&sign={}",
        url_encode(text),
        from_lang,
        to_lang,
        url_encode(appid.trim()),
        salt,
        sign
    );

    let req = HttpRequest::post(
        "https://fanyi-api.baidu.com/api/trans/vip/translate",
        body_str.into_bytes(),
    )
    .with_header("Content-Type", "application/x-www-form-urlencoded");

    let resp = host::http_request(req).map_err(|e| format!("百度翻译请求失败: {e}"))?;
    if !resp.is_success() {
        return Err(format!("百度翻译返回 HTTP 错误码: {}", resp.status));
    }

    let body_text = resp.text().map_err(|e| format!("解析响应失败: {e}"))?;
    let parsed: BaiduResponse = serde_json::from_str(&body_text)
        .map_err(|e| format!("解析百度翻译 JSON 失败: {e}"))?;

    parsed.into_text()
}

pub fn to_baidu_lang(lang: &str) -> &'static str {
    match lang {
        "auto" => "auto",
        "zh" | "zh-CN" | "zh-Hans" => "zh",
        "en" => "en",
        "ja" | "jp" => "jp",
        "ko" | "kor" => "kor",
        "fr" | "fra" => "fra",
        "de" => "de",
        "es" | "spa" => "spa",
        "ru" => "ru",
        other => {
            if other.starts_with("zh") {
                "zh"
            } else {
                "auto"
            }
        }
    }
}

#[derive(Debug, Deserialize)]
struct BaiduResponse {
    #[serde(default)]
    trans_result: Option<Vec<BaiduTransItem>>,
    #[serde(default)]
    error_code: Option<serde_json::Value>,
    #[serde(default)]
    error_msg: Option<String>,
}

#[derive(Debug, Deserialize)]
struct BaiduTransItem {
    dst: String,
}

impl BaiduResponse {
    fn into_text(self) -> Result<String, String> {
        if let Some(code_val) = &self.error_code {
            let code_str = match code_val {
                serde_json::Value::Number(n) => n.to_string(),
                serde_json::Value::String(s) => s.clone(),
                _ => String::new(),
            };
            if !code_str.is_empty() && code_str != "52000" {
                let msg = match code_str.as_str() {
                    "52001" => "请求超时，请重试".to_string(),
                    "52002" => "百度系统错误，请稍后重试".to_string(),
                    "52003" => "未授权用户：请检查 AppID 是否正确或开通服务".to_string(),
                    "54000" => "必填参数为空".to_string(),
                    "54001" => "签名错误：请检查 AppID 与 密钥 (Key) 是否匹配".to_string(),
                    "54003" => "访问频率受限，请稍后重试".to_string(),
                    "54004" => "账户余额不足".to_string(),
                    "54005" => "长请求频繁，请稍后重试".to_string(),
                    "58000" => "客户端 IP 非法".to_string(),
                    "58001" => "译文语言方向不支持".to_string(),
                    "58002" => "服务当前已关闭".to_string(),
                    _ => self.error_msg.unwrap_or_else(|| format!("错误码 {code_str}")),
                };
                return Err(format!("百度翻译错误 [{code_str}]: {msg}"));
            }
        }
        if let Some(items) = self.trans_result {
            let texts: Vec<String> = items.into_iter().map(|i| i.dst).collect();
            Ok(texts.join("\n"))
        } else {
            Err("百度翻译未返回翻译结果".to_string())
        }
    }
}

#[derive(Debug, Deserialize)]
struct MemoryResponse {
    #[serde(rename = "responseData")]
    response_data: Option<MemoryData>,
    #[serde(rename = "responseStatus")]
    response_status: Option<serde_json::Value>,
}

#[derive(Debug, Deserialize)]
struct MemoryData {
    #[serde(rename = "translatedText")]
    translated_text: Option<String>,
}

impl MemoryResponse {
    fn into_text(self) -> Result<String, String> {
        if let Some(data) = self.response_data {
            if let Some(txt) = data.translated_text {
                return Ok(html_unescape(&txt));
            }
        }
        let status = self
            .response_status
            .map(|s| s.to_string())
            .unwrap_or_default();
        Err(format!("MyMemory 翻译未返回有效结果 (状态: {status})"))
    }
}

fn html_unescape(text: &str) -> String {
    text.replace("&quot;", "\"")
        .replace("&apos;", "'")
        .replace("&#39;", "'")
        .replace("&amp;", "&")
        .replace("&lt;", "<")
        .replace("&gt;", ">")
}

pub fn url_encode(input: &str) -> String {
    let mut out = String::with_capacity(input.len() * 3);
    for b in input.as_bytes() {
        match *b {
            b'a'..=b'z' | b'A'..=b'Z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*b as char);
            }
            b' ' => out.push('+'),
            _ => {
                out.push_str(&format!("%{:02X}", b));
            }
        }
    }
    out
}

pub fn md5_hex(input: &[u8]) -> String {
    let mut a: u32 = 0x67452301;
    let mut b: u32 = 0xefcdab89;
    let mut c: u32 = 0x98badcfe;
    let mut d: u32 = 0x10325476;

    let orig_len = input.len();
    let mut data = input.to_vec();
    data.push(0x80);
    while (data.len() % 64) != 56 {
        data.push(0x00);
    }
    let bit_len = (orig_len as u64) * 8;
    data.extend_from_slice(&bit_len.to_le_bytes());

    let s: [u32; 64] = [
        7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 7, 12, 17, 22, 5, 9, 14, 20, 5, 9, 14, 20,
        5, 9, 14, 20, 5, 9, 14, 20, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23, 4, 11, 16, 23,
        6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21, 6, 10, 15, 21,
    ];

    let k: [u32; 64] = [
        0xd76aa478, 0xe8c7b756, 0x242070db, 0xc1bdceee, 0xf57c0faf, 0x4787c62a, 0xa8304613,
        0xfd469501, 0x698098d8, 0x8b44f7af, 0xffff5bb1, 0x895cd7be, 0x6b901122, 0xfd987193,
        0xa679438e, 0x49b40821, 0xf61e2562, 0xc040b340, 0x265e5a51, 0xe9b6c7aa, 0xd62f105d,
        0x02441453, 0xd8a1e681, 0xe7d3fbc8, 0x21e1cde6, 0xc33707d6, 0xf4d50d87, 0x455a14ed,
        0xa9e3e905, 0xfcefa3f8, 0x676f02d9, 0x8d2a4c8a, 0xfffa3942, 0x8771f681, 0x6d9d6122,
        0xfde5380c, 0xa4beea44, 0x4bdecfa9, 0xf6bb4b60, 0xbebfbc70, 0x289b7ec6, 0xeaa127fa,
        0xd4ef3085, 0x04881d05, 0xd9d4d039, 0xe6db99e5, 0x1fa27cf8, 0xc4ac5665, 0xf4292244,
        0x432aff97, 0xab9423a7, 0xfc93a039, 0x655b59c3, 0x8f0ccc92, 0xffeff47d, 0x85845dd1,
        0x6fa87e4f, 0xfe2ce6e0, 0xa3014314, 0x4e0811a1, 0xf7537e82, 0xbd3af235, 0x2ad7d2bb,
        0xeb86d391,
    ];

    for chunk in data.chunks_exact(64) {
        let mut m = [0u32; 16];
        for i in 0..16 {
            m[i] = u32::from_le_bytes([
                chunk[i * 4],
                chunk[i * 4 + 1],
                chunk[i * 4 + 2],
                chunk[i * 4 + 3],
            ]);
        }

        let mut aa = a;
        let mut bb = b;
        let mut cc = c;
        let mut dd = d;

        for i in 0..64 {
            let (f, g) = match i {
                0..=15 => ((bb & cc) | ((!bb) & dd), i),
                16..=31 => ((dd & bb) | ((!dd) & cc), (5 * i + 1) % 16),
                32..=47 => (bb ^ cc ^ dd, (3 * i + 5) % 16),
                _ => (cc ^ (bb | (!dd)), (7 * i) % 16),
            };

            let temp = dd;
            dd = cc;
            cc = bb;
            let sum = aa
                .wrapping_add(f)
                .wrapping_add(k[i])
                .wrapping_add(m[g]);
            bb = bb.wrapping_add(sum.rotate_left(s[i]));
            aa = temp;
        }

        a = a.wrapping_add(aa);
        b = b.wrapping_add(bb);
        c = c.wrapping_add(cc);
        d = d.wrapping_add(dd);
    }

    let mut result = Vec::with_capacity(16);
    result.extend_from_slice(&a.to_le_bytes());
    result.extend_from_slice(&b.to_le_bytes());
    result.extend_from_slice(&c.to_le_bytes());
    result.extend_from_slice(&d.to_le_bytes());

    let mut hex = String::with_capacity(32);
    for byte in result {
        hex.push_str(&format!("{:02x}", byte));
    }
    hex
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_md5_vector() {
        assert_eq!(md5_hex(b""), "d41d8cd98f00b204e9800998ecf8427e");
        assert_eq!(md5_hex(b"a"), "0cc175b9c0f1b6a831c399e269772661");
        assert_eq!(md5_hex(b"abc"), "900150983cd24fb0d6963f7d28e17f72");
    }

    #[test]
    fn test_url_encode() {
        assert_eq!(url_encode("hello world!"), "hello+world%21");
        assert_eq!(url_encode("你好"), "%E4%BD%A0%E5%A5%BD");
    }

    #[test]
    fn test_html_unescape() {
        assert_eq!(html_unescape("&quot;hello&amp;world&quot;"), "\"hello&world\"");
    }

    #[test]
    fn test_to_baidu_lang() {
        assert_eq!(to_baidu_lang("zh-CN"), "zh");
        assert_eq!(to_baidu_lang("en"), "en");
        assert_eq!(to_baidu_lang("ja"), "jp");
        assert_eq!(to_baidu_lang("ko"), "kor");
    }

    #[test]
    fn test_swap_state() {
        let (s, d, in_txt, out_txt) = swap_state(
            1, // zh-CN
            1, // en
            "你好".to_string(),
            "Hello".to_string(),
        );
        assert_eq!(s, 2); // en in SOURCE_LANGS
        assert_eq!(d, 0); // zh-CN in TARGET_LANGS
        assert_eq!(in_txt, "Hello");
        assert_eq!(out_txt, "你好");
    }
}
