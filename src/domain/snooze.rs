#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub enum SnoozeState {
    Inactive,
    Indefinite,
    Active { until_iso: String },
    Expired,
}
