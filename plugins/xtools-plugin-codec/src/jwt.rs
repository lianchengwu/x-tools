use crate::codec_ops::{decode_base64, encode_base64_url};
use serde_json::Value;
use std::fmt::Write;
use xtools_sdk::host;

/// Parses a JWT token into formatted Header, Payload, Signature, and humanized Claims.
pub fn parse_jwt(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    let token = if let Some(stripped) = trimmed.strip_prefix("Bearer ") {
        stripped.trim()
    } else if let Some(stripped) = trimmed.strip_prefix("bearer ") {
        stripped.trim()
    } else {
        trimmed
    };

    if token.is_empty() {
        return Ok(String::new());
    }
    let parts: Vec<&str> = token.split('.').collect();
    if parts.len() < 2 || parts.len() > 3 {
        return Err(format!(
            "非法的 JWT 格式：需包含 2 或 3 个以 '.' 分割的部分 (Header.Payload[.Signature])，实际检测到 {} 个部分",
            parts.len()
        ));
    }

    let header_bytes = decode_base64(parts[0])
        .map_err(|e| format!("Header Base64URL 解码失败: {e}"))?;
    let header_str = String::from_utf8(header_bytes)
        .map_err(|_| "Header 解码后不是合法 UTF-8 文本".to_string())?;
    let header_json: Value = serde_json::from_str(&header_str)
        .map_err(|e| format!("Header JSON 解析失败: {e}"))?;
    let header_pretty = serde_json::to_string_pretty(&header_json)
        .unwrap_or_else(|_| header_str.clone());

    let payload_bytes = decode_base64(parts[1])
        .map_err(|e| format!("Payload Base64URL 解码失败: {e}"))?;
    let payload_str = String::from_utf8(payload_bytes)
        .map_err(|_| "Payload 解码后不是合法 UTF-8 文本".to_string())?;
    let payload_json: Result<Value, _> = serde_json::from_str(&payload_str);
    let (payload_pretty, payload_val_opt) = match payload_json {
        Ok(val) => {
            let pretty = serde_json::to_string_pretty(&val).unwrap_or_else(|_| payload_str.clone());
            (pretty, Some(val))
        }
        Err(_) => (payload_str.clone(), None),
    };

    let signature_part = if parts.len() == 3 { parts[2] } else { "" };
    let signature_display = if signature_part.is_empty() {
        "(空 / 未签名 / alg: none)".to_string()
    } else {
        match decode_base64(signature_part) {
            Ok(sig_bytes) => {
                let hex_str: String = sig_bytes.iter().map(|b| format!("{b:02x}")).collect();
                format!("{signature_part}\nHex ({} 字节): {hex_str}", sig_bytes.len())
            }
            Err(_) => signature_part.to_string(),
        }
    };

    let claims_info = format_claims(&header_json, payload_val_opt.as_ref());

    let mut output = String::new();
    let _ = write!(output, "=== HEADER (标头) ===\n{header_pretty}\n\n");
    let _ = write!(output, "=== PAYLOAD (载荷) ===\n{payload_pretty}\n\n");
    let _ = write!(output, "=== SIGNATURE (签名) ===\n{signature_display}");

    if !claims_info.is_empty() {
        let _ = write!(output, "\n\n=== CLAIMS (声明分析) ===\n{claims_info}");
    }

    Ok(output)
}

/// Encodes JSON into a JWT token (unsigned / none algorithm by default, or preserves header/signature).
pub fn encode_jwt(input: &str) -> Result<String, String> {
    let trimmed = input.trim();
    if trimmed.is_empty() {
        return Ok(String::new());
    }

    if trimmed.contains("=== HEADER") || trimmed.contains("=== PAYLOAD") {
        return encode_from_sections(trimmed);
    }
    let json_val: Value = serde_json::from_str(trimmed)
        .map_err(|e| format!("JSON 解析失败: {e}。请输入合法的 JSON 载荷（Payload）或包含 header/payload 的对象"))?;

    let (header, payload, sig) = if let Value::Object(map) = &json_val {
        if map.contains_key("payload") && (map.contains_key("header") || map.len() <= 2) {
            let h = map.get("header").cloned().unwrap_or_else(default_header);
            let p = map.get("payload").cloned().unwrap_or(Value::Null);
            let s = map.get("signature").and_then(|v| v.as_str()).unwrap_or("").to_string();
            (h, p, s)
        } else {
            (default_header(), json_val, String::new())
        }
    } else {
        (default_header(), json_val, String::new())
    };

    let header_str = serde_json::to_string(&header)
        .map_err(|e| format!("Header 序列化失败: {e}"))?;
    let payload_str = serde_json::to_string(&payload)
        .map_err(|e| format!("Payload 序列化失败: {e}"))?;
    let h_b64 = encode_base64_url(header_str.as_bytes());
    let p_b64 = encode_base64_url(payload_str.as_bytes());

    if sig.is_empty() {
        Ok(format!("{h_b64}.{p_b64}."))
    } else {
        Ok(format!("{h_b64}.{p_b64}.{sig}"))
    }
}

fn encode_from_sections(text: &str) -> Result<String, String> {
    let mut header_raw = None;
    let mut payload_raw = None;
    let mut signature_raw = None;

    let lines: Vec<&str> = text.lines().collect();
    let mut current_sec = None;
    let mut current_content = String::new();

    let flush = |sec: Option<&str>, content: &str, h: &mut Option<String>, p: &mut Option<String>, s: &mut Option<String>| {
        if let Some(s_name) = sec {
            let trimmed = content.trim().to_string();
            if s_name.contains("HEADER") {
                *h = Some(trimmed);
            } else if s_name.contains("PAYLOAD") {
                *p = Some(trimmed);
            } else if s_name.contains("SIGNATURE") {
                *s = Some(trimmed);
            }
        }
    };

    for line in lines {
        if line.starts_with("===") {
            flush(current_sec, &current_content, &mut header_raw, &mut payload_raw, &mut signature_raw);
            current_content.clear();
            current_sec = Some(line);
        } else {
            current_content.push_str(line);
            current_content.push('\n');
        }
    }
    flush(current_sec, &current_content, &mut header_raw, &mut payload_raw, &mut signature_raw);

    let header_val: Value = if let Some(h) = header_raw {
        serde_json::from_str(&h).map_err(|e| format!("=== HEADER === 解析失败: {e}"))?
    } else {
        default_header()
    };

    let payload_val: Value = if let Some(p) = payload_raw {
        serde_json::from_str(&p).map_err(|e| format!("=== PAYLOAD === 解析失败: {e}"))?
    } else {
        return Err("未找到 === PAYLOAD === 载荷数据".to_string());
    };

    let header_str = serde_json::to_string(&header_val)
        .map_err(|e| format!("Header 序列化失败: {e}"))?;
    let payload_str = serde_json::to_string(&payload_val)
        .map_err(|e| format!("Payload 序列化失败: {e}"))?;

    let h_b64 = encode_base64_url(header_str.as_bytes());
    let p_b64 = encode_base64_url(payload_str.as_bytes());

    let sig_clean = match signature_raw {
        Some(s) => {
            let first_line = s.lines().next().unwrap_or("").trim();
            if first_line.starts_with('(') {
                String::new()
            } else {
                first_line.to_string()
            }
        }
        None => String::new(),
    };

    if sig_clean.is_empty() {
        Ok(format!("{h_b64}.{p_b64}."))
    } else {
        Ok(format!("{h_b64}.{p_b64}.{sig_clean}"))
    }
}

fn default_header() -> Value {
    serde_json::json!({
        "alg": "none",
        "typ": "JWT"
    })
}

fn format_claims(header: &Value, payload: Option<&Value>) -> String {
    let mut lines = Vec::new();
    let now_secs = host::now_millis() / 1000;

    if let Some(alg) = header.get("alg").and_then(|v| v.as_str()) {
        lines.push(format!("• 算法 (alg): {alg}"));
    }
    if let Some(typ) = header.get("typ").and_then(|v| v.as_str()) {
        lines.push(format!("• 类型 (typ): {typ}"));
    }
    if let Some(kid) = header.get("kid").and_then(|v| v.as_str()) {
        lines.push(format!("• 密钥ID (kid): {kid}"));
    }

    if let Some(Value::Object(map)) = payload {
        if let Some(iss) = map.get("iss") {
            let val = iss.as_str().map(|s| s.to_string()).unwrap_or_else(|| iss.to_string());
            lines.push(format!("• 签发者 (iss): {val}"));
        }
        if let Some(sub) = map.get("sub") {
            let val = sub.as_str().map(|s| s.to_string()).unwrap_or_else(|| sub.to_string());
            lines.push(format!("• 主体/用户 (sub): {val}"));
        }
        if let Some(aud) = map.get("aud") {
            if let Some(arr) = aud.as_array() {
                let items: Vec<String> = arr
                    .iter()
                    .map(|v| v.as_str().map(|s| s.to_string()).unwrap_or_else(|| v.to_string()))
                    .collect();
                lines.push(format!("• 受众 (aud): {}", items.join(", ")));
            } else if let Some(s) = aud.as_str() {
                lines.push(format!("• 受众 (aud): {s}"));
            } else {
                lines.push(format!("• 受众 (aud): {aud}"));
            }
        }
        if let Some(iat) = map.get("iat") {
            if let Some(ts) = parse_timestamp(iat) {
                let utc = format_unix_timestamp(ts);
                if now_secs > 0 && now_secs >= ts {
                    let diff = (now_secs - ts) as u64;
                    lines.push(format!(
                        "• 签发时间 (iat): {iat} ({utc}) [签发于 {} 前]",
                        format_duration(diff)
                    ));
                } else {
                    lines.push(format!("• 签发时间 (iat): {iat} ({utc})"));
                }
            } else {
                lines.push(format!("• 签发时间 (iat): {iat}"));
            }
        }
        if let Some(exp) = map.get("exp") {
            if let Some(ts) = parse_timestamp(exp) {
                let utc = format_unix_timestamp(ts);
                if now_secs > 0 {
                    if now_secs > ts {
                        let diff = (now_secs - ts) as u64;
                        lines.push(format!(
                            "• 过期时间 (exp): {exp} ({utc}) [已过期 · 过期于 {} 前]",
                            format_duration(diff)
                        ));
                    } else {
                        let diff = (ts - now_secs) as u64;
                        lines.push(format!(
                            "• 过期时间 (exp): {exp} ({utc}) [有效 · 剩余 {}]",
                            format_duration(diff)
                        ));
                    }
                } else {
                    lines.push(format!("• 过期时间 (exp): {exp} ({utc})"));
                }
            } else {
                lines.push(format!("• 过期时间 (exp): {exp}"));
            }
        }
        if let Some(nbf) = map.get("nbf") {
            if let Some(ts) = parse_timestamp(nbf) {
                let utc = format_unix_timestamp(ts);
                if now_secs > 0 {
                    if now_secs < ts {
                        let diff = (ts - now_secs) as u64;
                        lines.push(format!(
                            "• 生效时间 (nbf): {nbf} ({utc}) [尚未生效 · 将于 {} 后生效]",
                            format_duration(diff)
                        ));
                    } else {
                        lines.push(format!("• 生效时间 (nbf): {nbf} ({utc}) [已生效]"));
                    }
                } else {
                    lines.push(format!("• 生效时间 (nbf): {nbf} ({utc})"));
                }
            } else {
                lines.push(format!("• 生效时间 (nbf): {nbf}"));
            }
        }
        if let Some(jti) = map.get("jti") {
            let val = jti.as_str().map(|s| s.to_string()).unwrap_or_else(|| jti.to_string());
            lines.push(format!("• JWT ID (jti): {val}"));
        }
    }

    if lines.is_empty() {
        String::new()
    } else {
        lines.join("\n")
    }
}

fn parse_timestamp(v: &Value) -> Option<i64> {
    let raw = if let Some(n) = v.as_i64() {
        Some(n)
    } else if let Some(f) = v.as_f64() {
        Some(f as i64)
    } else if let Some(s) = v.as_str() {
        s.parse::<i64>().ok()
    } else {
        None
    }?;

    if raw.abs() > 99_999_999_999 {
        Some(raw / 1000)
    } else {
        Some(raw)
    }
}

fn format_unix_timestamp(secs: i64) -> String {
    let days = secs.div_euclid(86400);
    let rem_secs = secs.rem_euclid(86400);
    let hour = rem_secs / 3600;
    let minute = (rem_secs % 3600) / 60;
    let second = rem_secs % 60;

    let z = days + 719468;
    let era = (if z >= 0 { z } else { z - 146096 }) / 146097;
    let doe = (z - era * 146097) as u32;
    let yoe = (doe - doe / 1029 + doe / 1461 - doe / 146096) / 365;
    let y = (yoe as i64) + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let y = if m <= 2 { y + 1 } else { y };
    format!("{y:04}-{m:02}-{d:02} {hour:02}:{minute:02}:{second:02} UTC")
}

fn format_duration(secs: u64) -> String {
    if secs < 60 {
        format!("{secs} 秒")
    } else if secs < 3600 {
        let mins = secs / 60;
        let s = secs % 60;
        if s == 0 {
            format!("{mins} 分钟")
        } else {
            format!("{mins} 分 {s} 秒")
        }
    } else if secs < 86400 {
        let hours = secs / 3600;
        let mins = (secs % 3600) / 60;
        if mins == 0 {
            format!("{hours} 小时")
        } else {
            format!("{hours} 小时 {mins} 分")
        }
    } else {
        let days = secs / 86400;
        let hours = (secs % 86400) / 3600;
        if hours == 0 {
            format!("{days} 天")
        } else {
            format!("{days} 天 {hours} 小时")
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE_JWT: &str = "eyJhbGciOiJIUzI1NiIsInR5cCI6IkpXVCJ9.eyJzdWIiOiIxMjM0NTY3ODkwIiwibmFtZSI6IkpvaG4gRG9lIiwiaWF0IjoxNTE2MjM5MDIyLCJleHAiOjE3MTYyMzkwMjJ9.SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c";

    #[test]
    fn parse_jwt_standard_success() {
        let parsed = parse_jwt(SAMPLE_JWT).unwrap();
        assert!(parsed.contains("=== HEADER (标头) ==="));
        assert!(parsed.contains("\"alg\": \"HS256\""));
        assert!(parsed.contains("=== PAYLOAD (载荷) ==="));
        assert!(parsed.contains("\"name\": \"John Doe\""));
        assert!(parsed.contains("=== SIGNATURE (签名) ==="));
        assert!(parsed.contains("SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"));
        assert!(parsed.contains("Hex (32 字节):"));
        assert!(parsed.contains("=== CLAIMS (声明分析) ==="));
        assert!(parsed.contains("算法 (alg): HS256"));
        assert!(parsed.contains("主体/用户 (sub): 1234567890"));
        assert!(parsed.contains("签发时间 (iat): 1516239022 (2018-01-18 01:30:22 UTC)"));
        assert!(parsed.contains("过期时间 (exp): 1716239022 (2024-05-20 21:03:42 UTC)"));
    }

    #[test]
    fn parse_jwt_with_bearer_prefix() {
        let with_bearer = format!("Bearer {SAMPLE_JWT}");
        let parsed = parse_jwt(&with_bearer).unwrap();
        assert!(parsed.contains("\"name\": \"John Doe\""));
    }

    #[test]
    fn parse_jwt_unsigned_two_parts() {
        let unsigned = "eyJhbGciOiJub25lIiwidHlwIjoiSldUIn0.eyJzdWIiOiIxMjM0NTY3ODkwIn0.";
        let parsed = parse_jwt(unsigned).unwrap();
        assert!(parsed.contains("(空 / 未签名 / alg: none)"));
        assert!(parsed.contains("\"sub\": \"1234567890\""));
    }

    #[test]
    fn parse_jwt_invalid_format() {
        assert!(parse_jwt("single_string_without_dots").is_err());
        assert!(parse_jwt("one.two.three.four.five").is_err());
        assert_eq!(parse_jwt("").unwrap(), "");
    }

    #[test]
    fn encode_jwt_from_payload_json() {
        let payload = r#"{"sub": "admin", "role": "superuser"}"#;
        let token = encode_jwt(payload).unwrap();
        assert_eq!(token.matches('.').count(), 2);
        let parsed = parse_jwt(&token).unwrap();
        assert!(parsed.contains("\"role\": \"superuser\""));
        assert!(parsed.contains("\"alg\": \"none\""));
    }

    #[test]
    fn encode_jwt_round_trip() {
        let parsed = parse_jwt(SAMPLE_JWT).unwrap();
        let encoded = encode_jwt(&parsed).unwrap();
        let parsed_again = parse_jwt(&encoded).unwrap();
        assert!(parsed_again.contains("\"name\": \"John Doe\""));
        assert!(parsed_again.contains("SflKxwRJSMeKKF2QT4fwpMeJf36POk6yJV_adQssw5c"));
    }
}
