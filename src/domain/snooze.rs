use time::{
    format_description::well_known::Iso8601, macros::format_description, OffsetDateTime,
    PrimitiveDateTime,
};

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SnoozeState {
    Inactive,
    Indefinite,
    Active { until_iso: String },
    Expired,
    Malformed,
}

pub fn evaluate_snooze(iso_string: Option<&str>, now_iso: &str) -> SnoozeState {
    let Some(iso_string) = iso_string else {
        return SnoozeState::Inactive;
    };
    if iso_string == "indefinite" {
        return SnoozeState::Indefinite;
    }

    let Ok(now) = parse_iso_as_utc(now_iso) else {
        return SnoozeState::Malformed;
    };
    let Ok(until) = parse_iso_as_utc(iso_string) else {
        return SnoozeState::Malformed;
    };

    if now >= until {
        SnoozeState::Expired
    } else {
        SnoozeState::Active {
            until_iso: format_utc(until),
        }
    }
}

fn parse_iso_as_utc(value: &str) -> Result<OffsetDateTime, ()> {
    match OffsetDateTime::parse(value, &Iso8601::DEFAULT) {
        Ok(datetime) => Ok(datetime.to_offset(time::UtcOffset::UTC)),
        Err(_) => PrimitiveDateTime::parse(value, &Iso8601::DEFAULT)
            .map(|datetime| datetime.assume_utc())
            .map_err(|_| ()),
    }
}

/// 根据分钟数计算暂停状态。`0` 表示无限期暂停。
pub fn compute_snooze_from_minutes(minutes: u32) -> SnoozeState {
    if minutes == 0 {
        return SnoozeState::Indefinite;
    }
    let until = OffsetDateTime::now_utc() + time::Duration::minutes(minutes as i64);
    let until_iso = until
        .format(&Iso8601::DEFAULT)
        .unwrap_or_default();
    SnoozeState::Active { until_iso }
}

fn format_utc(value: OffsetDateTime) -> String {
    let format = format_description!("[year]-[month]-[day]T[hour]:[minute]:[second]+00:00");
    value
        .to_offset(time::UtcOffset::UTC)
        .format(&format)
        .expect("UTC ISO formatting should be infallible")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_minutes_returns_indefinite() {
        assert_eq!(compute_snooze_from_minutes(0), SnoozeState::Indefinite);
    }

    #[test]
    fn non_zero_minutes_returns_active_with_future_iso() {
        let before = OffsetDateTime::now_utc();
        let state = compute_snooze_from_minutes(15);
        let after = OffsetDateTime::now_utc();

        match state {
            SnoozeState::Active { until_iso } => {
                let until = parse_iso_as_utc(&until_iso).unwrap();
                // until 应在 [before+15min, after+15min] 范围内
                let expected_lo = before + time::Duration::minutes(15);
                let expected_hi = after + time::Duration::minutes(15);
                assert!(until >= expected_lo - time::Duration::seconds(1));
                assert!(until <= expected_hi + time::Duration::seconds(1));
            }
            other => panic!("expected Active, got {:?}", other),
        }
    }
}
