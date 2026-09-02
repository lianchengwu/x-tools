//! 时间戳与日期时间转换引擎，支持高精度、时区与批量转换。

use std::time::{SystemTime, UNIX_EPOCH};
use jiff::civil::DateTime;
use jiff::tz::TimeZone;
use jiff::Timestamp;

pub const TIMEZONE_NAMES: &[(&str, i8)] = &[
    ("Asia/Shanghai", 8),
    ("UTC", 0),
    ("Asia/Tokyo", 9),
    ("America/New_York", -5),
    ("America/Los_Angeles", -8),
    ("Europe/London", 0),
    ("Europe/Paris", 1),
    ("Asia/Singapore", 8),
    ("Australia/Sydney", 10),
    ("Asia/Dubai", 4),
    ("America/Chicago", -6),
    ("Europe/Berlin", 1),
    ("Asia/Karachi", 5),
];

pub fn resolve_tz(index: usize) -> TimeZone {
    if let Some(&(iana, offset)) = TIMEZONE_NAMES.get(index) {
        if let Ok(tz) = TimeZone::get(iana) {
            return tz;
        }
        let off = jiff::tz::Offset::from_seconds(offset as i32 * 3600)
            .unwrap_or(jiff::tz::Offset::UTC);
        return TimeZone::fixed(off);
    }
    TimeZone::UTC
}

pub fn now_timestamp_secs() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

pub fn now_timestamp_millis() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

pub fn ts_to_datetime(ts_val: i64, is_ms: bool, tz: &TimeZone) -> Result<String, String> {
    let ts = if is_ms {
        Timestamp::from_millisecond(ts_val).map_err(|e| format!("毫秒时间戳无效: {e}"))?
    } else {
        Timestamp::from_second(ts_val).map_err(|e| format!("秒时间戳无效: {e}"))?
    };
    let zdt = ts.to_zoned(tz.clone());
    Ok(zdt.strftime("%Y-%m-%d %H:%M:%S").to_string())
}

pub fn datetime_to_ts(dt_str: &str, is_ms: bool, tz: &TimeZone) -> Result<i64, String> {
    let trimmed = dt_str.trim();
    if trimmed.is_empty() {
        return Err("请输入日期时间".to_string());
    }

    let parsed: DateTime = if trimmed.contains('.') {
        trimmed.parse().or_else(|_| {
            let s = trimmed.replace(' ', "T");
            s.parse()
        }).map_err(|e| format!("日期时间格式错误: {e}"))?
    } else {
        trimmed
            .parse::<DateTime>()
            .or_else(|_| {
                let s = trimmed.replace(' ', "T");
                s.parse::<DateTime>()
            })
            .or_else(|_| {
                let s = format!("{trimmed} 00:00:00");
                s.replace(' ', "T").parse::<DateTime>()
            })
            .map_err(|e| format!("日期时间格式错误: {e}"))?
    };

    let zdt = parsed.to_zoned(tz.clone()).map_err(|e| format!("时区解析失败: {e}"))?;
    let ts = zdt.timestamp();
    if is_ms {
        Ok(ts.as_millisecond())
    } else {
        Ok(ts.as_second())
    }
}

pub fn batch_convert(input: &str, mode: usize, tz: &TimeZone) -> String {
    let mut results = Vec::new();
    for line in input.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() {
            results.push(String::new());
            continue;
        }

        if mode == 0 {
            // 时间戳 -> 日期时间
            match trimmed.parse::<i64>() {
                Ok(val) => {
                    let is_ms = trimmed.len() > 11;
                    match ts_to_datetime(val, is_ms, tz) {
                        Ok(dt) => results.push(dt),
                        Err(_) => results.push(format!("[错误: 无效时间戳 {trimmed}]")),
                    }
                }
                Err(_) => results.push(format!("[错误: 解析失败 {trimmed}]")),
            }
        } else {
            // 日期时间 -> 时间戳
            match datetime_to_ts(trimmed, false, tz) {
                Ok(ts) => results.push(ts.to_string()),
                Err(_) => results.push(format!("[错误: 解析失败 {trimmed}]")),
            }
        }
    }
    results.join("\n")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_ts_to_datetime() {
        let tz = resolve_tz(0); // Asia/Shanghai
        let dt = ts_to_datetime(1700000000, false, &tz).unwrap();
        assert!(dt.contains("2023-11-15"));
    }

    #[test]
    fn test_datetime_to_ts() {
        let tz = resolve_tz(0);
        let ts = datetime_to_ts("2023-11-15 06:13:20", false, &tz).unwrap();
        assert_eq!(ts, 1700000000);
    }

    #[test]
    fn test_batch_convert() {
        let tz = resolve_tz(0);
        let batch = batch_convert("1700000000\n1700000001", 0, &tz);
        assert!(batch.contains("2023-11-15"));
    }
}
