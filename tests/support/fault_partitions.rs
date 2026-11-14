#![allow(dead_code)]
use agent_memory::codec::json::{self, Value};
use std::collections::{BTreeMap, BTreeSet};
type Result<T, E = String> = std::result::Result<T, E>;
#[derive(Debug, Clone)]
pub struct Group {
    pub name: String,
    pub operation: &'static str,
    pub mode: Option<&'static str>,
    pub phase: &'static str,
    pub count: usize,
    pub suites: Vec<String>,
}
pub fn groups() -> Vec<Group> {
    let mut groups = Vec::new();
    for (operation, mode) in [
        ("update", None),
        ("install", Some("copy")),
        ("install", Some("link")),
        ("uninstall", Some("copy")),
        ("uninstall", Some("link")),
    ] {
        for phase in ["before", "after"] {
            let name = mode.map_or_else(
                || format!("{operation}-write-{phase}"),
                |mode| format!("{operation}-write-{mode}-{phase}"),
            );
            let count = if operation == "uninstall" { 4 } else { 8 };
            let suites = (0..count)
                .map(|index| {
                    if index == 0 {
                        name.clone()
                    } else {
                        format!("{name}-part-{index}")
                    }
                })
                .collect();
            groups.push(Group {
                name,
                operation,
                mode,
                phase,
                count,
                suites,
            });
        }
    }
    groups
}
fn field<'a>(value: &'a Value, key: &str) -> Result<&'a Value> {
    value.get(key).ok_or_else(|| format!("Missing {key}"))
}
fn text<'a>(value: &'a Value, key: &str) -> Result<&'a str> {
    field(value, key)?
        .as_str()
        .ok_or_else(|| format!("Invalid {key}"))
}
fn integer(value: &Value, key: &str) -> Result<usize> {
    field(value, key)?
        .as_int()
        .and_then(|value| value.as_u64())
        .and_then(|value| usize::try_from(value).ok())
        .ok_or_else(|| format!("Invalid {key}"))
}
fn array<'a>(value: &'a Value, key: &str) -> Result<&'a [Value]> {
    field(value, key)?
        .as_array()
        .ok_or_else(|| format!("Invalid {key}"))
}
fn require(condition: bool, detail: &str) -> Result<()> {
    if condition {
        Ok(())
    } else {
        Err(detail.into())
    }
}
fn string(value: impl Into<String>) -> Value {
    Value::String(value.into())
}
fn number(value: usize) -> Value {
    Value::Int(value.into())
}
fn object(fields: impl IntoIterator<Item = (&'static str, Value)>) -> Value {
    Value::Object(
        fields
            .into_iter()
            .map(|(key, value)| (key.into(), value))
            .collect(),
    )
}
pub fn select<'a>(rows: &'a [Value], group: &str, partition: usize) -> Result<Vec<&'a Value>> {
    let group = groups()
        .into_iter()
        .find(|candidate| candidate.name == group)
        .ok_or("Invalid lifecycle fault group")?;
    require(partition < group.count, "Invalid lifecycle fault partition")?;
    for (index, row) in rows.iter().enumerate() {
        require(
            integer(row, "step")? == index,
            "Complete observed fault inventory must be contiguous",
        )?;
    }
    let selected = rows
        .iter()
        .enumerate()
        .filter_map(|(index, row)| (index % group.count == partition).then_some(row))
        .collect::<Vec<_>>();
    require(
        !selected.is_empty(),
        "Registered lifecycle fault partition cannot be empty",
    )?;
    Ok(selected)
}
pub fn emit(group: &str, observed: &[&Value], executed: &[String], artifact: &str) {
    let rows: Vec<_> = observed
        .iter()
        .map(|row| {
            object([
                ("step", field(row, "step").unwrap().clone()),
                ("point", field(row, "point").unwrap().clone()),
                ("kind", field(row, "kind").unwrap().clone()),
            ])
        })
        .collect();
    assert_eq!(
        executed,
        rows.iter()
            .map(|row| text(row, "point").unwrap().to_owned())
            .collect::<Vec<_>>()
    );
    let registered = groups()
        .into_iter()
        .find(|candidate| candidate.name == group)
        .unwrap();
    for index in 0..registered.count {
        let expected = select(&rows, group, index).unwrap();
        let proof = object([
            ("group", string(group)),
            ("index", number(index)),
            ("count", number(registered.count)),
            ("artifact_digest", string(artifact)),
            ("observed", Value::Array(rows.clone())),
            (
                "executed",
                Value::Array(
                    expected
                        .iter()
                        .map(|row| field(row, "point").unwrap().clone())
                        .collect(),
                ),
            ),
        ]);
        eprintln!(
            "{}",
            json::encode(
                &object([("lifecycle_fault_partition", proof)]),
                json::COMPACT_ASCII
            )
            .unwrap()
        );
    }
}
pub fn extract(stderr: &str, suite: &str) -> Result<Value> {
    let owner = groups()
        .into_iter()
        .find_map(|group| {
            group
                .suites
                .iter()
                .position(|name| name == suite)
                .map(|index| (group.name.clone(), index))
        })
        .ok_or("Unknown lifecycle suite")?;
    let matches = stderr
        .lines()
        .filter_map(|line| json::decode(line, Default::default()).ok())
        .filter_map(|value| value.get("lifecycle_fault_partition").cloned())
        .filter(|proof| {
            text(proof, "group") == Ok(owner.0.as_str()) && integer(proof, "index") == Ok(owner.1)
        })
        .collect::<Vec<_>>();
    require(
        matches.len() == 1,
        "Expected one actual lifecycle interruption proof",
    )?;
    matches
        .into_iter()
        .next()
        .ok_or("Missing partition proof".into())
}
pub fn coverage(suites: &[Value]) -> Result<Vec<Value>> {
    let mut by_name = BTreeMap::new();
    for suite in suites {
        by_name.insert(text(suite, "name")?, suite);
    }
    let mut summaries = Vec::new();
    for group in groups() {
        let mut observed: Option<&[Value]> = None;
        let mut artifact = None;
        let mut collected = Vec::new();
        for (index, suite_name) in group.suites.iter().enumerate() {
            let suite = by_name.get(suite_name.as_str()).ok_or_else(|| {
                format!("Missing successful interruption partition: {suite_name}")
            })?;
            require(
                text(suite, "status")? == "passed",
                "Unsuccessful interruption partition",
            )?;
            let proof = field(suite, "fault_partition")?;
            require(
                text(proof, "group")? == group.name
                    && integer(proof, "index")? == index
                    && integer(proof, "count")? == group.count,
                "Invalid lifecycle interruption partition binding",
            )?;
            let rows = array(proof, "observed")?;
            require(
                !rows.is_empty(),
                "Incomplete observed interruption inventory",
            )?;
            for (step, row) in rows.iter().enumerate() {
                let fields = row.as_object().ok_or("Invalid observed interruption row")?;
                require(
                    fields.len() == 3
                        && fields
                            .iter()
                            .all(|(key, _)| ["step", "point", "kind"].contains(&key.as_str())),
                    "Incomplete observed interruption row",
                )?;
                require(
                    integer(row, "step")? == step,
                    "Noncontiguous observed interruption inventory",
                )?;
                text(row, "point")?;
                text(row, "kind")?;
            }
            let digest = text(proof, "artifact_digest")?;
            require(
                digest.len() == 64
                    && digest
                        .bytes()
                        .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte)),
                "Missing actual artifact identity",
            )?;
            if let Some(previous) = observed {
                require(
                    previous == rows && artifact == Some(digest),
                    "Partitions observed different writes or runtime artifacts",
                )?;
            } else {
                observed = Some(rows);
                artifact = Some(digest);
            }
            let expected: Vec<_> = select(rows, &group.name, index)?
                .into_iter()
                .map(|row| field(row, "point").cloned())
                .collect::<Result<_>>()?;
            require(
                array(proof, "executed")? == expected,
                "Missing or extra actual interruption/recovery/retry",
            )?;
            collected.extend(
                expected
                    .into_iter()
                    .map(|value| {
                        value
                            .as_str()
                            .map(str::to_owned)
                            .ok_or("Invalid executed point".into())
                    })
                    .collect::<Result<Vec<_>>>()?,
            );
        }
        let rows = observed.ok_or("Missing observed inventory")?;
        let mut points = rows
            .iter()
            .map(|row| text(row, "point").map(str::to_owned))
            .collect::<Result<Vec<_>>>()?;
        require(
            points.iter().collect::<BTreeSet<_>>().len() == points.len(),
            "Duplicate observed interruption points",
        )?;
        points.sort();
        collected.sort();
        require(
            points == collected,
            "Lifecycle interruption coverage has omissions or duplicates",
        )?;
        summaries.push(object([
            ("group", string(group.name)),
            ("partitions", number(group.count)),
            ("observed_points", number(points.len())),
            ("actual_interruptions_and_retries", number(collected.len())),
            (
                "artifact_digest",
                string(artifact.ok_or("Missing artifact")?),
            ),
        ]));
    }
    Ok(summaries)
}
