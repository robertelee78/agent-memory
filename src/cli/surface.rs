//! The declared command surface: generated from the parser table as one
//! canonical JSON value (signed into the release manifest), and the
//! `pack_validation::CommandSurface` provider built from such a declaration.
//! Update compatibility compares declarations; nothing is executed.

use super::parser::{Action, Arg, Default, Parser};
use super::{Entry, Helper, table};
use crate::codec::json::{self, Value};
use crate::lifecycle::{object, string};
use crate::pack_validation::{self, CommandRoute, CommandSurface, Dispatch, Invalid, Inventory};
use std::collections::{BTreeMap, BTreeSet};

pub const SCHEMA: &str = "agent-memory/command-surface-v1";
/// The root parser's baseline helper (installation.main).
pub const ROOT_SCRIPT: &str = "installer.py";
pub const ROOT_MODULE: &str = "installation";

fn strings<'a>(values: impl IntoIterator<Item = &'a str>) -> Value {
    Value::Array(values.into_iter().map(string).collect())
}

fn nullable(value: Option<&str>) -> Value {
    value.map_or(Value::Null, string)
}

fn parser_value(parser: &Parser<'_>) -> Value {
    let arguments = parser
        .args
        .iter()
        .map(|arg| match arg {
            Arg::Opt(opt) => object([
                ("kind", string("option")),
                ("flags", strings(opt.flags.iter().copied())),
                ("dest", string(opt.dest)),
                (
                    "action",
                    string(match opt.action {
                        Action::Store => "store",
                        Action::StoreTrue => "store_true",
                        Action::Append => "append",
                    }),
                ),
                ("required", Value::Bool(opt.required)),
                (
                    "default",
                    match (opt.action, opt.default) {
                        (Action::StoreTrue, _) => Value::Bool(false),
                        (_, Default::None) => Value::Null,
                        (_, Default::Text(text)) => string(text),
                        (_, Default::Empty) => Value::Array(Vec::new()),
                    },
                ),
                (
                    "choices",
                    opt.choices.values().map_or(Value::Null, |values| {
                        Value::Array(values.into_iter().map(string).collect())
                    }),
                ),
                ("help", nullable(opt.help)),
            ]),
            Arg::Pos(pos) => object([
                ("kind", string("positional")),
                ("dest", string(pos.dest)),
                ("choices", strings(pos.choices.iter().copied())),
                ("help", nullable(pos.help)),
            ]),
        })
        .collect();
    let exclusive = parser
        .exclusive
        .iter()
        .map(|group| {
            object([
                ("required", Value::Bool(group.required)),
                ("dests", strings(group.dests.iter().copied())),
            ])
        })
        .collect();
    let subcommands = parser.subcommands.as_ref().map_or(Value::Null, |subs| {
        object([
            ("dest", string(subs.dest)),
            ("required", Value::Bool(subs.required)),
            (
                "commands",
                Value::Array(
                    subs.commands
                        .iter()
                        .map(|command| {
                            object([
                                ("name", string(command.name)),
                                ("help", nullable(command.help)),
                                ("parser", parser_value(command.parser)),
                            ])
                        })
                        .collect(),
                ),
            ),
        ])
    });
    object([
        ("description", nullable(parser.description)),
        ("arguments", Value::Array(arguments)),
        ("exclusive", Value::Array(exclusive)),
        ("subcommands", subcommands),
    ])
}

fn route_value(name: &str, helper: &Helper, prefix: &[&str]) -> Value {
    object([
        ("name", string(name)),
        ("helper", string(helper.script)),
        ("prefix", strings(prefix.iter().copied())),
    ])
}

fn collect_helpers(helper: &'static Helper, seen: &mut Vec<&'static Helper>) {
    if seen.iter().any(|known| known.script == helper.script) {
        return;
    }
    seen.push(helper);
    for route in helper.routes {
        collect_helpers(route.helper, seen);
    }
}

/// The canonical declaration of every command, flag and route of `am`.
pub fn declaration() -> Value {
    let mut helpers = Vec::new();
    let commands = table::ENTRIES
        .iter()
        .map(|entry| match entry {
            Entry::Route(route) => {
                collect_helpers(route.helper, &mut helpers);
                object([
                    ("kind", string("route")),
                    ("route", route_value(route.name, route.helper, route.prefix)),
                ])
            }
            Entry::Native(native) => object([
                ("kind", string("native")),
                ("name", string(native.name)),
                ("help", string(native.help)),
                ("parser", parser_value(native.parser)),
            ]),
        })
        .collect();
    let helpers = helpers
        .iter()
        .map(|helper| {
            object([
                ("script", string(helper.script)),
                ("module", string(helper.module)),
                ("parser", parser_value(helper.parser)),
                (
                    "routes",
                    Value::Array(
                        helper
                            .routes
                            .iter()
                            .map(|route| route_value(route.name, route.helper, route.prefix))
                            .collect(),
                    ),
                ),
            ])
        })
        .collect();
    object([
        ("schema", string(SCHEMA)),
        ("program", string(crate::console::NAMES[0])),
        (
            "aliases",
            strings(crate::console::NAMES.iter().skip(1).copied()),
        ),
        (
            "root",
            object([
                ("script", string(ROOT_SCRIPT)),
                ("module", string(ROOT_MODULE)),
                ("description", string(super::ROOT_DESCRIPTION)),
            ]),
        ),
        ("commands", Value::Array(commands)),
        ("helpers", Value::Array(helpers)),
    ])
}

/// Canonical bytes for signing: sorted keys, compact, UTF-8.
pub fn canonical_bytes(declaration: &Value) -> crate::error::Result<Vec<u8>> {
    Ok(json::encode(declaration, json::SORTED_COMPACT_UTF8)?.into_bytes())
}

fn malformed(detail: &str) -> Invalid {
    Invalid::new(
        "command-surface",
        ".",
        format!("Malformed command surface: {detail}"),
    )
}

fn field<'a>(value: &'a Value, key: &str) -> pack_validation::Result<&'a Value> {
    value
        .as_object()
        .and(value.get(key))
        .ok_or_else(|| malformed(key))
}

fn text<'a>(value: &'a Value, key: &str) -> pack_validation::Result<&'a str> {
    field(value, key)?.as_str().ok_or_else(|| malformed(key))
}

fn array<'a>(value: &'a Value, key: &str) -> pack_validation::Result<&'a [Value]> {
    field(value, key)?.as_array().ok_or_else(|| malformed(key))
}

fn text_items<'a>(value: &'a Value, key: &str) -> pack_validation::Result<Vec<&'a str>> {
    array(value, key)?
        .iter()
        .map(|item| item.as_str().ok_or_else(|| malformed(key)))
        .collect()
}

/// Every subcommand name and operation choice, and every option string, of a
/// declared parser tree (the baseline's static dispatch extraction).
fn walk(
    parser: &Value,
    commands: &mut BTreeSet<String>,
    flags: &mut BTreeSet<String>,
    depth: usize,
) -> pack_validation::Result<()> {
    if depth > 8 {
        return Err(malformed("parser nesting exceeds its bound"));
    }
    for argument in array(parser, "arguments")? {
        match text(argument, "kind")? {
            "option" => {
                for flag in text_items(argument, "flags")? {
                    if !flag.starts_with('-') {
                        return Err(malformed("flags"));
                    }
                    flags.insert(flag.to_owned());
                }
            }
            "positional" => {
                if matches!(text(argument, "dest")?, "operation" | "command") {
                    commands.extend(
                        text_items(argument, "choices")?
                            .into_iter()
                            .map(str::to_owned),
                    );
                }
            }
            _ => return Err(malformed("kind")),
        }
    }
    let subcommands = field(parser, "subcommands")?;
    if subcommands != &Value::Null {
        for command in array(subcommands, "commands")? {
            commands.insert(text(command, "name")?.to_owned());
            walk(field(command, "parser")?, commands, flags, depth + 1)?;
        }
    }
    Ok(())
}

fn routes(values: &[Value]) -> pack_validation::Result<BTreeMap<String, CommandRoute>> {
    let mut routes = BTreeMap::new();
    for route in values {
        let helper = text(route, "helper")?;
        let prefix = text_items(route, "prefix")?;
        let valid_helper = helper.strip_suffix(".py").is_some_and(|stem| {
            stem.bytes().next().is_some_and(|b| b.is_ascii_lowercase())
                && stem
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_')
        });
        if !valid_helper || prefix.len() > 1 {
            return Err(malformed("route"));
        }
        let previous = routes.insert(
            text(route, "name")?.to_owned(),
            CommandRoute {
                helper: helper.to_owned(),
                prefix: prefix.into_iter().map(str::to_owned).collect(),
            },
        );
        if previous.is_some() {
            return Err(malformed("duplicate route"));
        }
    }
    Ok(routes)
}

const HELP: [&str; 2] = ["-h", "--help"];

/// A `CommandSurface` built only from a declaration value.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Declared {
    dispatches: BTreeMap<String, Dispatch>,
    commands: BTreeMap<String, (Dispatch, Vec<String>)>,
}

impl Declared {
    pub fn from_value(declaration: &Value) -> pack_validation::Result<Self> {
        if text(declaration, "schema")? != SCHEMA {
            return Err(malformed("schema"));
        }
        let mut dispatches = BTreeMap::new();
        for helper in array(declaration, "helpers")? {
            let mut commands = BTreeSet::new();
            let mut flags: BTreeSet<String> = HELP.iter().map(|f| (*f).to_owned()).collect();
            walk(field(helper, "parser")?, &mut commands, &mut flags, 0)?;
            let routes = routes(array(helper, "routes")?)?;
            let dispatch = Dispatch {
                module: format!("scripts/agent_memory/{}.py", text(helper, "module")?),
                commands,
                flags,
                routes,
            };
            if dispatches
                .insert(format!("scripts/{}", text(helper, "script")?), dispatch)
                .is_some()
            {
                return Err(malformed("duplicate helper"));
            }
        }
        let root = field(declaration, "root")?;
        let mut commands = BTreeSet::new();
        let mut flags: BTreeSet<String> = HELP.iter().map(|f| (*f).to_owned()).collect();
        let mut root_routes = Vec::new();
        let mut public_commands = BTreeMap::new();
        for command in array(declaration, "commands")? {
            let (name, dispatch, prefix) = match text(command, "kind")? {
                "route" => {
                    let route = field(command, "route")?;
                    let name = text(route, "name")?;
                    root_routes.push(route.clone());
                    let dispatch = dispatches
                        .get(&format!("scripts/{}", text(route, "helper")?))
                        .ok_or_else(|| malformed("route helper"))?
                        .clone();
                    let prefix = text_items(route, "prefix")?
                        .into_iter()
                        .map(str::to_owned)
                        .collect();
                    (name, dispatch, prefix)
                }
                "native" => {
                    let name = text(command, "name")?;
                    let mut operations = BTreeSet::new();
                    let mut options: BTreeSet<String> =
                        HELP.iter().map(|flag| (*flag).to_owned()).collect();
                    walk(field(command, "parser")?, &mut operations, &mut options, 1)?;
                    flags.extend(options.iter().cloned());
                    let dispatch = Dispatch {
                        module: text(root, "module")?.to_owned(),
                        commands: operations,
                        flags: options,
                        routes: BTreeMap::new(),
                    };
                    (name, dispatch, Vec::new())
                }
                _ => return Err(malformed("kind")),
            };
            commands.insert(name.to_owned());
            if public_commands
                .insert(name.to_owned(), (dispatch, prefix))
                .is_some()
            {
                return Err(malformed("duplicate command"));
            }
        }
        let root_dispatch = Dispatch {
            module: format!("scripts/agent_memory/{}.py", text(root, "module")?),
            commands,
            flags,
            routes: routes(&root_routes)?,
        };
        if dispatches
            .insert(format!("scripts/{}", text(root, "script")?), root_dispatch)
            .is_some()
        {
            return Err(malformed("duplicate helper"));
        }
        for dispatch in dispatches.values() {
            for (name, route) in &dispatch.routes {
                if !dispatch.commands.contains(name) {
                    return Err(malformed("route outside commands"));
                }
                let child = dispatches
                    .get(&format!("scripts/{}", route.helper))
                    .ok_or_else(|| malformed("route helper"))?;
                if route
                    .prefix
                    .first()
                    .is_some_and(|operation| !child.commands.contains(operation))
                {
                    return Err(malformed("route prefix"));
                }
            }
        }
        Ok(Self {
            dispatches,
            commands: public_commands,
        })
    }

    /// The provider for the running binary's own table.
    pub fn current() -> pack_validation::Result<Self> {
        Self::from_value(&declaration())
    }

    pub fn scripts(&self) -> impl Iterator<Item = &str> {
        self.dispatches.keys().map(String::as_str)
    }
}

impl CommandSurface for Declared {
    fn dispatch(&self, script: &str, _files: &Inventory) -> pack_validation::Result<Dispatch> {
        let dispatch = self.dispatches.get(script).ok_or_else(|| {
            Invalid::new(
                "dispatch",
                script,
                "Helper is not declared by the signed command surface",
            )
        })?;
        Ok(dispatch.clone())
    }

    fn command(&self, name: &str) -> pack_validation::Result<Option<(Dispatch, Vec<String>)>> {
        Ok(self.commands.get(name).cloned())
    }
}
