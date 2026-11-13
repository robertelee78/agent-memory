//! Bounded, body-free canonical bank capacity observations.
use crate::codec::json::Value;
use crate::error::{ContractError, Kind, Result};
use crate::lifecycle::{object, string};
use crate::sharing_limits::{MAX_BANK, MAX_FILE, MAX_FILES};
use std::fmt;

pub const REPAIR: &str = "Preserve the bank, feature work, original index/HEAD and all recovery evidence. Ordinary recall and new capture remain pending. Use am bank recover-layout inspect with a private output path for explicit source-preserving review. Do not truncate, delete, skip a source, raise normal limits or initialize over existing memory; repeating the same operation does not repair capacity.";
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CapacityCode {
    SourceBytes,
    AggregateBytes,
    SourceCount,
}
impl CapacityCode {
    pub fn code(self) -> &'static str {
        match self {
            Self::SourceBytes => "bank-source-byte-capacity",
            Self::AggregateBytes => "bank-aggregate-byte-capacity",
            Self::SourceCount => "bank-source-count-capacity",
        }
    }
    pub fn unit(self) -> &'static str {
        match self {
            Self::SourceCount => "sources",
            _ => "bytes",
        }
    }
    fn dimension(self) -> &'static str {
        match self {
            Self::SourceBytes => "source-byte",
            Self::AggregateBytes => "aggregate-byte",
            Self::SourceCount => "source-count",
        }
    }
}
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Measurement {
    Exact,
    AtLeast,
}
impl Measurement {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Exact => "exact",
            Self::AtLeast => "at-least",
        }
    }
}
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BankCapacityError {
    pub code: CapacityCode,
    pub path: String,
    pub limit: usize,
    pub observed: usize,
    pub measurement: Measurement,
    pub unit: &'static str,
}
impl BankCapacityError {
    pub fn new(
        code: CapacityCode,
        path: &str,
        limit: usize,
        observed: usize,
        measurement: Measurement,
    ) -> Result<Self> {
        if path.is_empty()
            || path.starts_with('/')
            || (path != "."
                && path
                    .split('/')
                    .any(|p| p.is_empty() || p == "." || p == ".."))
            || path.chars().any(|c| c < ' ' || c == '\u{7f}')
            || observed <= limit
        {
            return Err(ContractError::new(
                "Invalid canonical capacity observation; preserve sources",
            ));
        }
        Ok(Self {
            code,
            path: path.into(),
            limit,
            observed,
            measurement,
            unit: code.unit(),
        })
    }
    pub fn details(&self) -> Value {
        object([
            ("failure_code", string(self.code.code())),
            ("failure_phase", string("bank-eligibility")),
            ("failure_component", string("canonical-bank")),
            ("failure_path", string(&self.path)),
            ("capacity_limit", Value::Int(self.limit.into())),
            ("capacity_observed", Value::Int(self.observed.into())),
            ("capacity_measurement", string(self.measurement.as_str())),
            ("capacity_unit", string(self.unit)),
            ("repair", string(REPAIR)),
        ])
    }
}
impl fmt::Display for BankCapacityError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "Canonical bank {} capacity exceeded: {} {}; limit {}.",
            self.code.dimension(),
            self.measurement.as_str(),
            self.observed,
            self.limit
        )
    }
}
impl std::error::Error for BankCapacityError {}
impl From<BankCapacityError> for ContractError {
    fn from(e: BankCapacityError) -> Self {
        let message = e.to_string();
        ContractError::bank_capacity(
            e.code,
            e.path.into(),
            e.limit,
            e.observed,
            e.measurement,
            message,
        )
    }
}
pub fn details(error: &ContractError) -> Value {
    match error.kind() {
        Kind::BankCapacity {
            code,
            path,
            limit,
            observed,
            measurement,
            unit,
        } => BankCapacityError {
            code: *code,
            path: path.to_string_lossy().into_owned(),
            limit: *limit,
            observed: *observed,
            measurement: *measurement,
            unit: *unit,
        }
        .details(),
        _ => object(Vec::<(String, Value)>::new()),
    }
}
pub fn check_files(files: &Value) -> Result<()> {
    let files = files
        .as_object()
        .ok_or_else(|| ContractError::new("A complete canonical file map is required"))?;
    if files.len() > MAX_FILES {
        return Err(BankCapacityError::new(
            CapacityCode::SourceCount,
            ".",
            MAX_FILES,
            files.len(),
            Measurement::Exact,
        )?
        .into());
    }
    let mut total = 0usize;
    for (path, body) in files {
        let amount = body
            .as_str()
            .ok_or_else(|| ContractError::new("Canonical file content must be UTF-8 text"))?
            .len();
        if amount > MAX_FILE {
            return Err(BankCapacityError::new(
                CapacityCode::SourceBytes,
                path,
                MAX_FILE,
                amount,
                Measurement::Exact,
            )?
            .into());
        }
        total += amount;
    }
    if total > MAX_BANK {
        return Err(BankCapacityError::new(
            CapacityCode::AggregateBytes,
            ".",
            MAX_BANK,
            total,
            Measurement::Exact,
        )?
        .into());
    }
    Ok(())
}
