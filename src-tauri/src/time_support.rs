use jiff::{civil::DateTime, ZonedDateTime};

pub(crate) fn local_now() -> String {
    ZonedDateTime::now()
        .strftime("%Y-%m-%d %H:%M:%S")
        .to_string()
}

#[cfg(any(feature = "desktop", test))]
pub(crate) fn local_now_rfc3339() -> String {
    ZonedDateTime::now()
        .strftime("%Y-%m-%dT%H:%M:%S%.f%:z")
        .to_string()
}

pub(crate) fn parse_history_datetime(value: &str) -> Result<DateTime, jiff::Error> {
    DateTime::strptime("%Y-%m-%d %H:%M:%S%.f", value)
}

#[cfg(any(feature = "desktop", test))]
pub(crate) fn history_timestamp(value: &str, zone: &jiff::tz::TimeZone) -> Option<i64> {
    if let Ok(timestamp) = value.parse::<jiff::Timestamp>() {
        return Some(timestamp.as_second());
    }
    let datetime = parse_history_datetime(value).ok()?;
    // Legacy records have no offset. Do not invent an instant at DST transitions.
    zone.to_ambiguous_timestamp(datetime)
        .unambiguous()
        .ok()
        .map(|timestamp| timestamp.as_second())
}

pub(crate) fn retention_cutoff(now: &ZonedDateTime, days: u32) -> Result<String, String> {
    // Preserve the previous fixed 24-hour retention interval across DST changes.
    now.checked_sub(jiff::SignedDuration::from_secs(i64::from(days) * 86_400))
        .map(|cutoff| cutoff.strftime("%Y-%m-%d %H:%M:%S").to_string())
        .map_err(|_| "历史保留期超出支持的日期范围".to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use jiff::tz::TimeZone;

    #[test]
    fn legacy_history_preserves_seconds_and_nanoseconds() {
        for value in [
            "2026-11-13 12:00:00",
            "2026-11-13 12:00:00.123",
            "2026-11-13 12:00:00.123456789",
        ] {
            assert!(parse_history_datetime(value).is_ok(), "{value}");
        }
        for value in ["invalid", "2026-02-29 12:00:00", "2026-11-13 24:00:00"] {
            assert!(parse_history_datetime(value).is_err(), "{value}");
        }
    }

    #[test]
    fn history_timestamp_respects_offsets_and_legacy_local_time() {
        let zone = TimeZone::get("Asia/Shanghai").unwrap();
        let expected = "2026-11-13T12:00:00+08:00"
            .parse::<jiff::Timestamp>()
            .unwrap()
            .as_second();
        for value in [
            "2026-11-13 12:00:00",
            "2026-11-13 12:00:00.123456789",
            "2026-11-13T12:00:00+08:00",
            "2026-11-13T04:00:00Z",
        ] {
            assert_eq!(history_timestamp(value, &zone), Some(expected), "{value}");
        }
        assert_eq!(history_timestamp("invalid", &zone), None);
    }

    #[test]
    fn legacy_history_rejects_dst_gaps_and_overlaps() {
        let zone = TimeZone::get("America/New_York").unwrap();
        assert_eq!(history_timestamp("2026-03-08 02:30:00", &zone), None);
        assert_eq!(history_timestamp("2026-11-01 01:30:00", &zone), None);
        assert!(history_timestamp("2026-03-08 03:30:00", &zone).is_some());
        assert!(history_timestamp("2026-11-01T01:30:00-04:00", &zone).is_some());
        assert!(history_timestamp("2026-11-01T01:30:00-05:00", &zone).is_some());
    }

    #[test]
    fn retention_uses_elapsed_days_and_rejects_overflow() {
        let now = "2026-03-08T03:30:00-04:00[America/New_York]"
            .parse::<ZonedDateTime>()
            .unwrap();
        assert_eq!(retention_cutoff(&now, 1).unwrap(), "2026-03-07 02:30:00");
        assert!(retention_cutoff(&now, u32::MAX).is_err());
    }

    #[test]
    fn generated_times_keep_existing_storage_and_backup_formats() {
        assert_eq!(local_now().len(), 19);
        assert!(parse_history_datetime(&local_now()).is_ok());
        let rfc3339 = local_now_rfc3339();
        assert!(rfc3339.parse::<jiff::Timestamp>().is_ok());
        assert!(!rfc3339.contains('['));
    }
}
