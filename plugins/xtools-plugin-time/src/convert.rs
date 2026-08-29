use jiff::Timestamp;
use jiff::civil::DateTime;
use jiff::tz::TimeZone;

pub const LOCAL_FMT: &str = "%Y-%m-%d %H:%M:%S%.3f";
pub const SECOND_FMT: &str = "%Y-%m-%d %H:%M:%S";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct TimezoneOption {
    pub label: &'static str,
    pub iana_name: Option<&'static str>,
    pub fallback_offset_hours: i8,
}

pub const TIMEZONE_OPTIONS: &[TimezoneOption] = &[
    TimezoneOption {
        label: "中国标准时间 (UTC+8)",
        iana_name: Some("Asia/Shanghai"),
        fallback_offset_hours: 8,
    },
    TimezoneOption {
        label: "协调世界时 (UTC)",
        iana_name: Some("UTC"),
        fallback_offset_hours: 0,
    },
    TimezoneOption {
        label: "日本标准时间 (JST, UTC+9)",
        iana_name: Some("Asia/Tokyo"),
        fallback_offset_hours: 9,
    },
    TimezoneOption {
        label: "美国东部时间 (ET, UTC-5/4)",
        iana_name: Some("America/New_York"),
        fallback_offset_hours: -5,
    },
    TimezoneOption {
        label: "美国太平洋时间 (PT, UTC-8/7)",
        iana_name: Some("America/Los_Angeles"),
        fallback_offset_hours: -8,
    },
    TimezoneOption {
        label: "格林威治标准时间 (GMT, UTC+0)",
        iana_name: Some("Europe/London"),
        fallback_offset_hours: 0,
    },
    TimezoneOption {
        label: "中欧时间 (CET, UTC+1/2)",
        iana_name: Some("Europe/Paris"),
        fallback_offset_hours: 1,
    },
    TimezoneOption {
        label: "澳大利亚东部时间 (AEST, UTC+10)",
        iana_name: Some("Australia/Sydney"),
        fallback_offset_hours: 10,
    },
    TimezoneOption {
        label: "新加坡时间 (SGT, UTC+8)",
        iana_name: Some("Asia/Singapore"),
        fallback_offset_hours: 8,
    },
    TimezoneOption {
        label: "海湾标准时间 (GST, UTC+4)",
        iana_name: Some("Asia/Dubai"),
        fallback_offset_hours: 4,
    },
    TimezoneOption {
        label: "德国时间 (CET/CEST, UTC+1/2)",
        iana_name: Some("Europe/Berlin"),
        fallback_offset_hours: 1,
    },
    TimezoneOption {
        label: "美国中部时间 (CT, UTC-6/5)",
        iana_name: Some("America/Chicago"),
        fallback_offset_hours: -6,
    },
    TimezoneOption {
        label: "巴基斯坦标准时间 (PKT, UTC+5)",
        iana_name: Some("Asia/Karachi"),
        fallback_offset_hours: 5,
    },
];

pub fn resolve_timezone_by_index(index: usize) -> TimeZone {
    TIMEZONE_OPTIONS
        .get(index)
        .map(resolve_timezone)
        .unwrap_or_else(|| TimeZone::UTC)
}

pub fn resolve_timezone(opt: &TimezoneOption) -> TimeZone {
    if let Some(iana) = opt.iana_name {
        if let Ok(tz) = TimeZone::get(iana) {
            return tz;
        }
    }
    let offset = jiff::tz::Offset::from_seconds(opt.fallback_offset_hours as i32 * 3600)
        .unwrap_or(jiff::tz::Offset::UTC);
    TimeZone::fixed(offset)
}

pub fn from_now(tz: &TimeZone) -> (i64, i64, String) {
    let now_ms = crate::host::now_millis();
    let ts = Timestamp::from_millisecond(now_ms).unwrap_or(Timestamp::UNIX_EPOCH);
    format_ts(ts, tz)
}

pub fn from_seconds(s: i64, tz: &TimeZone) -> Result<(i64, i64, String), jiff::Error> {
    Ok(format_ts(Timestamp::from_second(s)?, tz))
}

pub fn from_millis(ms: i64, tz: &TimeZone) -> Result<(i64, i64, String), jiff::Error> {
    Ok(format_ts(Timestamp::from_millisecond(ms)?, tz))
}

pub fn from_datetime(text: &str, tz: &TimeZone) -> Result<(i64, i64, String), jiff::Error> {
    let trimmed = text.trim();
    if trimmed.is_empty() {
        return Err(jiff::Error::from(
            jiff::fmt::temporal::DateTimeParser::new().parse_date(trimmed).unwrap_err(),
        ));
    }

    let parsed: DateTime = if trimmed.contains('.') {
        trimmed.parse().or_else(|_| {
            let s = trimmed.replace(' ', "T");
            s.parse()
        })?
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
            })?
    };

    let zdt = parsed.to_zoned(tz.clone())?;
    let ts = zdt.timestamp();
    Ok(format_ts(ts, tz))
}

pub fn format_ts(ts: Timestamp, tz: &TimeZone) -> (i64, i64, String) {
    let zdt = ts.to_zoned(tz.clone());
    let local = zdt.strftime(LOCAL_FMT).to_string();
    (ts.as_second(), ts.as_millisecond(), local)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timezone_resolve() {
        let tz0 = resolve_timezone_by_index(0);
        let tz1 = resolve_timezone_by_index(1);
        let ts = Timestamp::from_second(1700000000).unwrap();
        let (_, _, s0) = format_ts(ts, &tz0);
        let (_, _, s1) = format_ts(ts, &tz1);
        assert_ne!(s0, s1);
        assert!(s0.contains("2023-11-15"));
    }

    #[test]
    fn test_from_seconds_and_millis() {
        let tz = resolve_timezone_by_index(0);
        let (s, ms, dt) = from_seconds(1700000000, &tz).unwrap();
        assert_eq!(s, 1700000000);
        assert_eq!(ms, 1700000000000);
        assert!(dt.contains("2023-11-15 06:13:20"));

        let (s2, ms2, dt2) = from_millis(1700000000123, &tz).unwrap();
        assert_eq!(s2, 1700000000);
        assert_eq!(ms2, 1700000000123);
        assert!(dt2.contains(".123"));
    }

    #[test]
    fn test_from_datetime() {
        let tz = resolve_timezone_by_index(0);
        let (s, ms, _) = from_datetime("2023-11-15 06:13:20", &tz).unwrap();
        assert_eq!(s, 1700000000);
        assert_eq!(ms, 1700000000000);

        let (s2, ms2, _) = from_datetime("2023-11-15 06:13:20.500", &tz).unwrap();
        assert_eq!(s2, 1700000000);
        assert_eq!(ms2, 1700000000500);

        let (s3, _, _) = from_datetime("2023-11-15", &tz).unwrap();
        assert!(s3 > 0);
    }
}
