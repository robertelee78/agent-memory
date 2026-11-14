//! Declarative command parser reproducing the baseline argparse contract.
//!
//! Accepted flags, unique long-option prefixes, `--opt=value`, the `--`
//! marker, choices, required options, mutually exclusive groups, exit codes
//! and error messages follow CPython 3.13 argparse (vectors in
//! `tests/vectors/cli-argparse-v1.json`). Usage and help use argparse's
//! layout at a fixed 80-column terminal; that layout is deterministic here and
//! not a byte contract.

use crate::console;

/// Terminal width argparse would derive from COLUMNS=80 (`COLUMNS - 2`).
const WIDTH: usize = 78;
/// argparse `max_help_position` for the fixed width.
const MAX_HELP_POSITION: usize = 24;
const HELP_FLAGS: &[&str] = &["-h", "--help"];
const HELP_TEXT: &str = "show this help message and exit";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Action {
    Store,
    StoreTrue,
    Append,
}

#[derive(Clone, Copy)]
pub enum Choices {
    Any,
    Fixed(&'static [&'static str]),
    /// Choices computed from an embedded schema (for example record policy fields).
    Computed(fn() -> Vec<String>),
}

impl Choices {
    pub fn values(&self) -> Option<Vec<String>> {
        match self {
            Self::Any => None,
            Self::Fixed(values) => Some(values.iter().map(|v| (*v).to_owned()).collect()),
            Self::Computed(values) => Some(values()),
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Default {
    /// argparse `None`.
    None,
    Text(&'static str),
    /// `action='append', default=[]`.
    Empty,
}

#[derive(Clone, Copy)]
pub struct Opt {
    pub flags: &'static [&'static str],
    pub dest: &'static str,
    pub action: Action,
    pub required: bool,
    pub default: Default,
    pub choices: Choices,
    pub help: Option<&'static str>,
}

impl Opt {
    const fn new(flags: &'static [&'static str], dest: &'static str, action: Action) -> Self {
        Self {
            flags,
            dest,
            action,
            required: false,
            default: Default::None,
            choices: Choices::Any,
            help: None,
        }
    }
    /// `add_argument(flags…, dest=dest)` storing one value.
    pub const fn store(flags: &'static [&'static str], dest: &'static str) -> Self {
        Self::new(flags, dest, Action::Store)
    }
    /// `action='store_true'`.
    pub const fn flag(flags: &'static [&'static str], dest: &'static str) -> Self {
        Self::new(flags, dest, Action::StoreTrue)
    }
    /// `action='append'` without a default (absent means None).
    pub const fn append(flags: &'static [&'static str], dest: &'static str) -> Self {
        Self::new(flags, dest, Action::Append)
    }
    pub const fn required(mut self) -> Self {
        self.required = true;
        self
    }
    pub const fn default(mut self, value: &'static str) -> Self {
        self.default = Default::Text(value);
        self
    }
    /// `default=[]` for append options.
    pub const fn empty(mut self) -> Self {
        self.default = Default::Empty;
        self
    }
    pub const fn choices(mut self, values: &'static [&'static str]) -> Self {
        self.choices = Choices::Fixed(values);
        self
    }
    pub const fn computed(mut self, values: fn() -> Vec<String>) -> Self {
        self.choices = Choices::Computed(values);
        self
    }
    pub const fn help(mut self, text: &'static str) -> Self {
        self.help = Some(text);
        self
    }
}

/// A required single positional (`nargs=None`), optionally with choices.
#[derive(Clone, Copy)]
pub struct Positional {
    pub dest: &'static str,
    pub choices: &'static [&'static str],
    pub help: Option<&'static str>,
}

#[derive(Clone, Copy)]
pub enum Arg {
    Opt(Opt),
    Pos(Positional),
}

/// `add_mutually_exclusive_group(required=…)`; members are option dests.
#[derive(Clone, Copy)]
pub struct Exclusive {
    pub required: bool,
    pub dests: &'static [&'static str],
}

pub struct Subcommand<'a> {
    pub name: &'static str,
    pub help: Option<&'a str>,
    pub parser: &'a Parser<'a>,
}

/// `add_subparsers(dest=…, required=…)`. Declared before any other argument
/// of its parser, as every baseline parser does.
pub struct Subcommands<'a> {
    pub dest: &'static str,
    pub required: bool,
    pub commands: &'a [Subcommand<'a>],
}

pub struct Parser<'a> {
    pub description: Option<&'static str>,
    pub args: &'a [Arg],
    pub exclusive: &'a [Exclusive],
    pub subcommands: Option<Subcommands<'a>>,
}

impl Parser<'static> {
    pub const EMPTY: Parser<'static> = Parser {
        description: None,
        args: &[],
        exclusive: &[],
        subcommands: None,
    };
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Value {
    Absent,
    Flag(bool),
    Text(String),
    List(Vec<String>),
}

/// The parsed namespace of one parser level plus the selected subcommand.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Matches {
    values: Vec<(&'static str, Value)>,
    command: Option<(&'static str, Box<Matches>)>,
}

impl Matches {
    fn get(&self, dest: &str) -> Option<&Value> {
        self.values
            .iter()
            .find(|(name, _)| *name == dest)
            .map(|(_, value)| value)
    }
    /// A store or positional value; None when absent with no default.
    pub fn text(&self, dest: &str) -> Option<&str> {
        match self.get(dest) {
            Some(Value::Text(value)) => Some(value),
            _ => None,
        }
    }
    pub fn flag(&self, dest: &str) -> bool {
        matches!(self.get(dest), Some(Value::Flag(true)))
    }
    /// An append value; None when absent without `default=[]`.
    pub fn list(&self, dest: &str) -> Option<&[String]> {
        match self.get(dest) {
            Some(Value::List(values)) => Some(values),
            _ => None,
        }
    }
    pub fn command(&self) -> Option<(&'static str, &Matches)> {
        self.command
            .as_ref()
            .map(|(name, sub)| (*name, sub.as_ref()))
    }
}

/// Help or usage termination: argparse's SystemExit with its printed text.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Exit {
    pub code: i32,
    pub stdout: String,
    pub stderr: String,
}

/// Parse `args` with `parser` named `prog`, refusing unrecognized arguments
/// with this parser's usage, as `parse_args` does.
pub fn parse(parser: &Parser<'_>, prog: &str, args: &[String]) -> Result<Matches, Exit> {
    let mut extras = Vec::new();
    let matches = parse_known(parser, prog, args, &mut extras)?;
    if !extras.is_empty() {
        let message = format!("unrecognized arguments: {}", extras.join(" "));
        return Err(error(parser, prog, &message));
    }
    Ok(matches)
}

/// The error exit: usage on stderr, then `prog: error: message`, status 2.
pub fn error(parser: &Parser<'_>, prog: &str, message: &str) -> Exit {
    Exit {
        code: 2,
        stdout: String::new(),
        stderr: format!("{}{prog}: error: {message}\n", usage(parser, prog)),
    }
}

/// One action of a parser level in argparse `_actions` order.
#[derive(Clone, Copy)]
enum Item<'p, 'a> {
    Help,
    Commands(&'p Subcommands<'a>),
    Opt(&'p Opt),
    Pos(&'p Positional),
}

impl Item<'_, '_> {
    fn flags(&self) -> &'static [&'static str] {
        match self {
            Self::Help => HELP_FLAGS,
            Self::Opt(opt) => opt.flags,
            _ => &[],
        }
    }
    fn dest(&self) -> &'static str {
        match self {
            Self::Help => "help",
            Self::Commands(commands) => commands.dest,
            Self::Opt(opt) => opt.dest,
            Self::Pos(pos) => pos.dest,
        }
    }
    fn takes_value(&self) -> bool {
        matches!(self, Self::Opt(opt) if opt.action != Action::StoreTrue)
    }
    fn required(&self) -> bool {
        match self {
            Self::Help => false,
            Self::Commands(commands) => commands.required,
            Self::Opt(opt) => opt.required,
            Self::Pos(_) => true,
        }
    }
    /// argparse `_get_action_name`.
    fn name(&self) -> String {
        let flags = self.flags();
        if flags.is_empty() {
            self.dest().to_owned()
        } else {
            flags.join("/")
        }
    }
    fn same(&self, other: &Self) -> bool {
        self.dest() == other.dest() && self.flags() == other.flags()
    }
}

fn items<'p, 'a>(parser: &'p Parser<'a>) -> Vec<Item<'p, 'a>> {
    let mut items = vec![Item::Help];
    if let Some(commands) = &parser.subcommands {
        items.push(Item::Commands(commands));
    }
    for arg in parser.args {
        items.push(match arg {
            Arg::Opt(opt) => Item::Opt(opt),
            Arg::Pos(pos) => Item::Pos(pos),
        });
    }
    items
}

/// A classified option occurrence: the action, the matched option string and
/// an explicit argument with its separator (`=` or empty for `-xVALUE`).
#[derive(Clone)]
struct Occurrence<'p, 'a> {
    item: Option<Item<'p, 'a>>,
    flag: String,
    separator: Option<&'static str>,
    explicit: Option<String>,
}

/// argparse's arg-strings pattern: `A`, `-` (the first `--`) or `O` with
/// every candidate `_parse_optional` returned (more than one is ambiguous,
/// refused only when `consume_optional` reaches it).
enum Pattern<'p, 'a> {
    Positional,
    Marker,
    Optional(Vec<Occurrence<'p, 'a>>),
}

fn option_items<'p, 'a>(items: &[Item<'p, 'a>]) -> Vec<(&'static str, Item<'p, 'a>)> {
    let mut map = Vec::new();
    for item in items {
        for flag in item.flags() {
            map.push((*flag, *item));
        }
    }
    map
}

/// Unicode 15.1 (CPython 3.13 `unicodedata`) general category Nd, the
/// characters `re`'s `\d` matches in a str pattern: DerivedGeneralCategory
/// 16.0.0 without the seven ranges Unicode 16.0 added.
const DECIMAL_DIGITS: &[(u32, u32)] = &[
    (0x0030, 0x0039),
    (0x0660, 0x0669),
    (0x06F0, 0x06F9),
    (0x07C0, 0x07C9),
    (0x0966, 0x096F),
    (0x09E6, 0x09EF),
    (0x0A66, 0x0A6F),
    (0x0AE6, 0x0AEF),
    (0x0B66, 0x0B6F),
    (0x0BE6, 0x0BEF),
    (0x0C66, 0x0C6F),
    (0x0CE6, 0x0CEF),
    (0x0D66, 0x0D6F),
    (0x0DE6, 0x0DEF),
    (0x0E50, 0x0E59),
    (0x0ED0, 0x0ED9),
    (0x0F20, 0x0F29),
    (0x1040, 0x1049),
    (0x1090, 0x1099),
    (0x17E0, 0x17E9),
    (0x1810, 0x1819),
    (0x1946, 0x194F),
    (0x19D0, 0x19D9),
    (0x1A80, 0x1A89),
    (0x1A90, 0x1A99),
    (0x1B50, 0x1B59),
    (0x1BB0, 0x1BB9),
    (0x1C40, 0x1C49),
    (0x1C50, 0x1C59),
    (0xA620, 0xA629),
    (0xA8D0, 0xA8D9),
    (0xA900, 0xA909),
    (0xA9D0, 0xA9D9),
    (0xA9F0, 0xA9F9),
    (0xAA50, 0xAA59),
    (0xABF0, 0xABF9),
    (0xFF10, 0xFF19),
    (0x104A0, 0x104A9),
    (0x10D30, 0x10D39),
    (0x11066, 0x1106F),
    (0x110F0, 0x110F9),
    (0x11136, 0x1113F),
    (0x111D0, 0x111D9),
    (0x112F0, 0x112F9),
    (0x11450, 0x11459),
    (0x114D0, 0x114D9),
    (0x11650, 0x11659),
    (0x116C0, 0x116C9),
    (0x11730, 0x11739),
    (0x118E0, 0x118E9),
    (0x11950, 0x11959),
    (0x11C50, 0x11C59),
    (0x11D50, 0x11D59),
    (0x11DA0, 0x11DA9),
    (0x11F50, 0x11F59),
    (0x16A60, 0x16A69),
    (0x16AC0, 0x16AC9),
    (0x16B50, 0x16B59),
    (0x1D7CE, 0x1D7FF),
    (0x1E140, 0x1E149),
    (0x1E2F0, 0x1E2F9),
    (0x1E4F0, 0x1E4F9),
    (0x1E950, 0x1E959),
    (0x1FBF0, 0x1FBF9),
];

fn decimal_digit(c: char) -> bool {
    let code = u32::from(c);
    DECIMAL_DIGITS
        .binary_search_by(|(low, high)| {
            if *high < code {
                std::cmp::Ordering::Less
            } else if *low > code {
                std::cmp::Ordering::Greater
            } else {
                std::cmp::Ordering::Equal
            }
        })
        .is_ok()
}

/// argparse `_negative_number_matcher`: `re.match(r'^-\d+$|^-\d*\.\d+$')`,
/// where `\d` is any Nd character and `$` also matches before one final `\n`.
fn negative_number(arg: &str) -> bool {
    let Some(rest) = arg.strip_prefix('-') else {
        return false;
    };
    let rest = rest.strip_suffix('\n').unwrap_or(rest);
    let digits = |s: &str| s.chars().all(decimal_digit);
    match rest.split_once('.') {
        None => !rest.is_empty() && digits(rest),
        Some((whole, fraction)) => digits(whole) && !fraction.is_empty() && digits(fraction),
    }
}

/// argparse `_parse_optional` with `_get_option_tuples`: None for a
/// positional, otherwise every candidate (one `item: None` tuple for an
/// unknown option).
fn classify<'p, 'a>(
    arg: &str,
    options: &[(&'static str, Item<'p, 'a>)],
) -> Option<Vec<Occurrence<'p, 'a>>> {
    if !arg.starts_with('-') {
        return None;
    }
    if let Some((flag, item)) = options.iter().find(|(flag, _)| *flag == arg) {
        return Some(vec![Occurrence {
            item: Some(*item),
            flag: (*flag).to_owned(),
            separator: None,
            explicit: None,
        }]);
    }
    if arg.len() == 1 {
        return None;
    }
    let (prefix, explicit) = match arg.split_once('=') {
        Some((prefix, value)) => (prefix, Some(value)),
        None => (arg, None),
    };
    if let Some(value) = explicit {
        if let Some((flag, item)) = options.iter().find(|(flag, _)| *flag == prefix) {
            return Some(vec![Occurrence {
                item: Some(*item),
                flag: (*flag).to_owned(),
                separator: Some("="),
                explicit: Some(value.to_owned()),
            }]);
        }
    }
    let mut tuples = Vec::new();
    if arg.starts_with("--") {
        for (flag, item) in options {
            if flag.starts_with(prefix) {
                tuples.push(Occurrence {
                    item: Some(*item),
                    flag: (*flag).to_owned(),
                    separator: explicit.map(|_| "="),
                    explicit: explicit.map(str::to_owned),
                });
            }
        }
    } else {
        // `option_string[:2]` and the rest, by characters; prefixes are
        // compared with the part before `=`.
        let mut chars = arg.char_indices();
        let _ = chars.next();
        let split = chars
            .next()
            .map_or(arg.len(), |(index, c)| index + c.len_utf8());
        let (short, rest) = arg.split_at(split);
        for (flag, item) in options {
            if *flag == short {
                tuples.push(Occurrence {
                    item: Some(*item),
                    flag: (*flag).to_owned(),
                    separator: Some(""),
                    explicit: Some(rest.to_owned()),
                });
            } else if flag.starts_with(prefix) {
                tuples.push(Occurrence {
                    item: Some(*item),
                    flag: (*flag).to_owned(),
                    separator: explicit.map(|_| "="),
                    explicit: explicit.map(str::to_owned),
                });
            }
        }
    }
    if !tuples.is_empty() {
        return Some(tuples);
    }
    let negative_options = options.iter().any(|(flag, _)| negative_number(flag));
    if negative_number(arg) && !negative_options {
        return None;
    }
    if arg.contains(' ') {
        return None;
    }
    Some(vec![Occurrence {
        item: None,
        flag: arg.to_owned(),
        separator: None,
        explicit: None,
    }])
}

struct State<'p, 'a> {
    values: Vec<(&'static str, Value)>,
    command: Option<(&'static str, Box<Matches>)>,
    seen: Vec<Item<'p, 'a>>,
    seen_non_default: Vec<Item<'p, 'a>>,
}

fn check_choice(
    item: &Item<'_, '_>,
    value: &str,
    choices: Option<Vec<String>>,
) -> Result<(), String> {
    let Some(choices) = choices else {
        return Ok(());
    };
    if choices.iter().any(|choice| choice == value) {
        return Ok(());
    }
    Err(format!(
        "argument {}: invalid choice: {} (choose from {})",
        item.name(),
        console::repr(value),
        choices.join(", ")
    ))
}

fn parse_known(
    parser: &Parser<'_>,
    prog: &str,
    args: &[String],
    extras: &mut Vec<String>,
) -> Result<Matches, Exit> {
    let items = items(parser);
    let options = option_items(&items);
    let fail = |message: String| error(parser, prog, &message);
    let mut patterns = Vec::with_capacity(args.len());
    let mut marker = false;
    for arg in args {
        if marker {
            patterns.push(Pattern::Positional);
        } else if arg == "--" {
            marker = true;
            patterns.push(Pattern::Marker);
        } else {
            match classify(arg, &options) {
                Some(candidates) => patterns.push(Pattern::Optional(candidates)),
                None => patterns.push(Pattern::Positional),
            }
        }
    }
    let mut state = State {
        values: Vec::new(),
        command: None,
        seen: Vec::new(),
        seen_non_default: Vec::new(),
    };
    for item in &items {
        let value = match item {
            Item::Opt(opt) => match (opt.action, opt.default) {
                (Action::StoreTrue, _) => Value::Flag(false),
                (_, Default::Text(text)) => Value::Text(text.to_owned()),
                (_, Default::Empty) => Value::List(Vec::new()),
                (_, Default::None) => Value::Absent,
            },
            _ => Value::Absent,
        };
        if !matches!(item, Item::Help) {
            state.values.push((item.dest(), value));
        }
    }
    let mut positionals: Vec<Item<'_, '_>> = items
        .iter()
        .copied()
        .filter(|item| matches!(item, Item::Commands(_) | Item::Pos(_)))
        .collect();
    let option_indices: Vec<usize> = patterns
        .iter()
        .enumerate()
        .filter(|(_, pattern)| matches!(pattern, Pattern::Optional(_)))
        .map(|(index, _)| index)
        .collect();
    let max_option = option_indices.last().copied();
    let mut start = 0;
    while max_option.is_some_and(|max| start <= max) {
        let next_option = option_indices
            .iter()
            .copied()
            .find(|index| *index >= start)
            .unwrap_or(args.len());
        if start != next_option {
            let end = consume_positionals(
                parser,
                prog,
                args,
                &patterns,
                start,
                &mut positionals,
                &mut state,
                extras,
            )?;
            if end > start {
                start = end;
                continue;
            }
            start = end;
        }
        if !option_indices.contains(&start) {
            extras.extend(args.iter().take(next_option).skip(start).cloned());
            start = next_option;
        }
        start = consume_optional(parser, prog, args, &patterns, start, &mut state, extras)?;
    }
    let stop = consume_positionals(
        parser,
        prog,
        args,
        &patterns,
        start,
        &mut positionals,
        &mut state,
        extras,
    )?;
    extras.extend(args.iter().skip(stop).cloned());
    let missing: Vec<String> = items
        .iter()
        .filter(|item| item.required() && !state.seen.iter().any(|seen| seen.same(item)))
        .map(Item::name)
        .collect();
    if !missing.is_empty() {
        return Err(fail(format!(
            "the following arguments are required: {}",
            missing.join(", ")
        )));
    }
    for group in parser.exclusive.iter().filter(|group| group.required) {
        let members: Vec<Item<'_, '_>> = group_items(&items, group);
        let present = members
            .iter()
            .any(|member| state.seen_non_default.iter().any(|seen| seen.same(member)));
        if !present {
            let names: Vec<String> = members.iter().map(Item::name).collect();
            return Err(fail(format!(
                "one of the arguments {} is required",
                names.join(" ")
            )));
        }
    }
    Ok(Matches {
        values: state.values,
        command: state.command,
    })
}

fn group_items<'p, 'a>(items: &[Item<'p, 'a>], group: &Exclusive) -> Vec<Item<'p, 'a>> {
    group
        .dests
        .iter()
        .filter_map(|dest| {
            items
                .iter()
                .copied()
                .find(|item| matches!(item, Item::Opt(opt) if opt.dest == *dest))
        })
        .collect()
}

fn conflicts<'p, 'a>(
    parser: &Parser<'_>,
    items: &[Item<'p, 'a>],
    item: &Item<'p, 'a>,
) -> Vec<Item<'p, 'a>> {
    let mut result = Vec::new();
    for group in parser.exclusive {
        let members = group_items(items, group);
        if members.iter().any(|member| member.same(item)) {
            result.extend(members.into_iter().filter(|member| !member.same(item)));
        }
    }
    result
}

#[allow(clippy::too_many_arguments)]
fn consume_positionals<'p, 'a>(
    parser: &Parser<'_>,
    prog: &str,
    args: &[String],
    patterns: &[Pattern<'p, 'a>],
    start: usize,
    positionals: &mut Vec<Item<'p, 'a>>,
    state: &mut State<'p, 'a>,
    extras: &mut Vec<String>,
) -> Result<usize, Exit> {
    let fail = |message: String| error(parser, prog, &message);
    let mut index = start;
    while let Some(item) = positionals.first().copied() {
        // `-*A`: leading `--` markers, then one positional-class argument.
        let mut cursor = index;
        while matches!(patterns.get(cursor), Some(Pattern::Marker)) {
            cursor += 1;
        }
        if !matches!(patterns.get(cursor), Some(Pattern::Positional)) {
            break;
        }
        let Some(value) = args.get(cursor) else {
            break;
        };
        positionals.remove(0);
        record_seen(parser, item, state).map_err(fail)?;
        match item {
            Item::Commands(commands) => {
                let names: Vec<String> = commands
                    .commands
                    .iter()
                    .map(|command| command.name.to_owned())
                    .collect();
                // argparse 3.12+: a leading `--` before a PARSER positional is
                // removed, so the argument after it is the choice.
                let selected = value.as_str();
                check_choice(&item, selected, Some(names)).map_err(fail)?;
                let Some(command) = commands
                    .commands
                    .iter()
                    .find(|command| command.name == selected)
                else {
                    break;
                };
                set(state, commands.dest, Value::Text(command.name.to_owned()));
                let sub_prog = format!("{prog} {}", command.name);
                let rest = args.get(cursor + 1..).unwrap_or(&[]);
                let matches = parse_known(command.parser, &sub_prog, rest, extras)?;
                state.command = Some((command.name, Box::new(matches)));
                return Ok(args.len());
            }
            Item::Pos(pos) => {
                let choices = if pos.choices.is_empty() {
                    None
                } else {
                    Some(pos.choices.iter().map(|c| (*c).to_owned()).collect())
                };
                check_choice(&item, value, choices).map_err(fail)?;
                set(state, pos.dest, Value::Text(value.clone()));
                cursor += 1;
                // Trailing `--` markers belong to the same match (`-*A-*`).
                while matches!(patterns.get(cursor), Some(Pattern::Marker)) {
                    cursor += 1;
                }
                index = cursor;
            }
            Item::Help | Item::Opt(_) => break,
        }
    }
    Ok(index)
}

fn set(state: &mut State<'_, '_>, dest: &'static str, value: Value) {
    if let Some(slot) = state.values.iter_mut().find(|(name, _)| *name == dest) {
        slot.1 = value;
    } else {
        state.values.push((dest, value));
    }
}

/// argparse `take_action` bookkeeping: an explicit occurrence is never the
/// default object, so it conflicts with any earlier member of its group.
fn record_seen<'p, 'a>(
    parser: &Parser<'_>,
    item: Item<'p, 'a>,
    state: &mut State<'p, 'a>,
) -> Result<(), String> {
    state.seen.push(item);
    let items = items(parser);
    for conflict in conflicts(parser, &items, &item) {
        if state
            .seen_non_default
            .iter()
            .any(|seen| seen.same(&conflict))
        {
            return Err(format!(
                "argument {}: not allowed with argument {}",
                item.name(),
                conflict.name()
            ));
        }
    }
    state.seen_non_default.push(item);
    Ok(())
}

fn consume_optional<'p, 'a>(
    parser: &Parser<'_>,
    prog: &str,
    args: &[String],
    patterns: &[Pattern<'p, 'a>],
    start: usize,
    state: &mut State<'p, 'a>,
    extras: &mut Vec<String>,
) -> Result<usize, Exit> {
    let fail = |message: String| error(parser, prog, &message);
    let Some(Pattern::Optional(candidates)) = patterns.get(start) else {
        return Ok(start + 1);
    };
    let occurrence = match candidates.as_slice() {
        [occurrence] => occurrence,
        _ => {
            let names = candidates
                .iter()
                .map(|candidate| candidate.flag.as_str())
                .collect::<Vec<_>>()
                .join(", ");
            let arg = args.get(start).map_or("", String::as_str);
            return Err(fail(format!("ambiguous option: {arg} could match {names}")));
        }
    };
    let Some(item) = occurrence.item else {
        if let Some(arg) = args.get(start) {
            extras.push(arg.clone());
        }
        return Ok(start + 1);
    };
    let argument = |message: &str| format!("argument {}: {message}", item.name());
    let (value, stop) = match &occurrence.explicit {
        Some(explicit) => {
            if !item.takes_value() {
                let short = occurrence.flag.as_bytes().get(1) != Some(&b'-');
                if short
                    && !explicit.is_empty()
                    && occurrence.separator == Some("")
                    && !explicit.starts_with('-')
                {
                    // Combined short flags: `-hx` keeps `-x` as an extra. No
                    // baseline parser declares a second short option.
                    extras.push(format!("-{explicit}"));
                    (None, start + 1)
                } else {
                    return Err(fail(argument(&format!(
                        "ignored explicit argument {}",
                        console::repr(explicit)
                    ))));
                }
            } else {
                (Some(explicit.clone()), start + 1)
            }
        }
        None if item.takes_value() => match (patterns.get(start + 1), args.get(start + 1)) {
            (Some(Pattern::Positional), Some(value)) => (Some(value.clone()), start + 2),
            _ => return Err(fail(argument("expected one argument"))),
        },
        None => (None, start + 1),
    };
    take(parser, prog, item, value, state)?;
    Ok(stop)
}

fn take<'p, 'a>(
    parser: &Parser<'_>,
    prog: &str,
    item: Item<'p, 'a>,
    value: Option<String>,
    state: &mut State<'p, 'a>,
) -> Result<(), Exit> {
    let fail = |message: String| error(parser, prog, &message);
    if let (Item::Opt(opt), Some(text)) = (item, value.as_deref()) {
        check_choice(&item, text, opt.choices.values()).map_err(fail)?;
    }
    record_seen(parser, item, state).map_err(fail)?;
    match item {
        Item::Help => {
            return Err(Exit {
                code: 0,
                stdout: help(parser, prog),
                stderr: String::new(),
            });
        }
        Item::Opt(opt) => match opt.action {
            Action::StoreTrue => set(state, opt.dest, Value::Flag(true)),
            Action::Store => set(state, opt.dest, value.map_or(Value::Absent, Value::Text)),
            Action::Append => {
                let mut list = match state.values.iter().find(|(name, _)| *name == opt.dest) {
                    Some((_, Value::List(values))) => values.clone(),
                    _ => Vec::new(),
                };
                list.extend(value);
                set(state, opt.dest, Value::List(list));
            }
        },
        Item::Commands(_) | Item::Pos(_) => {}
    }
    Ok(())
}

fn metavar_choices(choices: &[String]) -> String {
    format!("{{{}}}", choices.join(","))
}

/// argparse `_format_args` for one value.
fn metavar(item: &Item<'_, '_>) -> String {
    match item {
        Item::Help => String::new(),
        Item::Commands(commands) => {
            let names: Vec<String> = commands
                .commands
                .iter()
                .map(|c| c.name.to_owned())
                .collect();
            metavar_choices(&names)
        }
        Item::Pos(pos) if !pos.choices.is_empty() => {
            let names: Vec<String> = pos.choices.iter().map(|c| (*c).to_owned()).collect();
            metavar_choices(&names)
        }
        Item::Pos(pos) => pos.dest.to_owned(),
        Item::Opt(opt) => match opt.choices.values() {
            Some(values) => metavar_choices(&values),
            None => opt.dest.to_uppercase(),
        },
    }
}

fn usage_part(item: &Item<'_, '_>, grouped: bool) -> String {
    match item {
        Item::Commands(_) => format!("{} ...", metavar(item)),
        Item::Pos(_) => metavar(item),
        Item::Help | Item::Opt(_) => {
            let flag = item.flags().first().copied().unwrap_or_default();
            let part = if item.takes_value() {
                format!("{flag} {}", metavar(item))
            } else {
                flag.to_owned()
            };
            if item.required() || grouped {
                part
            } else {
                format!("[{part}]")
            }
        }
    }
}

/// argparse `_get_actions_usage_parts`: optionals then positionals, with
/// contiguous mutually exclusive groups bracketed and separated by `|`.
fn usage_parts(parser: &Parser<'_>) -> (Vec<String>, usize) {
    let all = items(parser);
    let mut ordered: Vec<Item<'_, '_>> = all
        .iter()
        .copied()
        .filter(|item| !item.flags().is_empty())
        .collect();
    let optional_count = ordered.len();
    ordered.extend(all.iter().copied().filter(|item| item.flags().is_empty()));
    let mut grouped = vec![false; ordered.len()];
    let mut inserts = Vec::new();
    for group in parser.exclusive {
        let members = group_items(&all, group);
        let Some(first) = members.first() else {
            continue;
        };
        let Some(begin) = ordered.iter().position(|item| item.same(first)) else {
            continue;
        };
        let end = begin + members.len();
        let contiguous = ordered.get(begin..end).is_some_and(|slice| {
            slice
                .iter()
                .zip(&members)
                .all(|(left, right)| left.same(right))
        });
        if contiguous {
            for flag in grouped.iter_mut().take(end).skip(begin) {
                *flag = true;
            }
            inserts.push((begin, end, group.required));
        }
    }
    let mut parts: Vec<Option<String>> = ordered
        .iter()
        .zip(&grouped)
        .map(|(item, grouped)| Some(usage_part(item, *grouped)))
        .collect();
    inserts.sort_by(|left, right| right.cmp(left));
    for (begin, end, required) in inserts {
        let size = end - begin;
        let (open, close) = match (required, size > 1) {
            (true, true) => ("(", ")"),
            (true, false) => ("", ""),
            (false, _) => ("[", "]"),
        };
        for (offset, slot) in parts.iter_mut().take(end).skip(begin).enumerate() {
            if let Some(part) = slot.as_mut() {
                if offset == 0 {
                    part.insert_str(0, open);
                }
                if offset + 1 == size {
                    part.push_str(close);
                } else {
                    part.push_str(" |");
                }
            }
        }
    }
    // Every part is kept (groups never suppress members), so positional parts
    // begin exactly after the optional parts.
    let result: Vec<String> = parts.into_iter().flatten().collect();
    (result, optional_count)
}

fn wrap_parts(parts: &[String], indent: &str, prefix: Option<&str>) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line: Vec<&str> = Vec::new();
    let mut length = match prefix {
        Some(prefix) => prefix.len().saturating_sub(1),
        None => indent.len().saturating_sub(1),
    };
    for part in parts {
        if length + 1 + part.chars().count() > WIDTH && !line.is_empty() {
            lines.push(format!("{indent}{}", line.join(" ")));
            line.clear();
            length = indent.len().saturating_sub(1);
        }
        line.push(part);
        length += part.chars().count() + 1;
    }
    if !line.is_empty() {
        lines.push(format!("{indent}{}", line.join(" ")));
    }
    if prefix.is_some() {
        if let Some(first) = lines.first_mut() {
            if let Some(rest) = first.strip_prefix(indent) {
                *first = rest.to_owned();
            }
        }
    }
    lines
}

/// `usage: …` followed by a newline, argparse `format_usage` layout.
pub fn usage(parser: &Parser<'_>, prog: &str) -> String {
    const PREFIX: &str = "usage: ";
    let (parts, positional_start) = usage_parts(parser);
    let mut text = prog.to_owned();
    for part in &parts {
        text.push(' ');
        text.push_str(part);
    }
    if PREFIX.len() + text.chars().count() > WIDTH {
        let (optionals, positionals) = parts.split_at(positional_start.min(parts.len()));
        let prog_length = prog.chars().count();
        let lines = if (PREFIX.len() + prog_length) * 4 <= WIDTH * 3 {
            let indent = " ".repeat(PREFIX.len() + prog_length + 1);
            let mut first = vec![prog.to_owned()];
            if !optionals.is_empty() {
                first.extend_from_slice(optionals);
                let mut lines = wrap_parts(&first, &indent, Some(PREFIX));
                lines.extend(wrap_parts(positionals, &indent, None));
                lines
            } else if !positionals.is_empty() {
                first.extend_from_slice(positionals);
                wrap_parts(&first, &indent, Some(PREFIX))
            } else {
                first
            }
        } else {
            let indent = " ".repeat(PREFIX.len());
            let mut lines = wrap_parts(&parts, &indent, None);
            if lines.len() > 1 {
                lines = wrap_parts(optionals, &indent, None);
                lines.extend(wrap_parts(positionals, &indent, None));
            }
            let mut all = vec![prog.to_owned()];
            all.extend(lines);
            all
        };
        text = lines.join("\n");
    }
    format!("{PREFIX}{text}\n")
}

/// Whitespace-normalized greedy word wrap (textwrap without hyphen breaks).
fn wrap_words(text: &str, width: usize) -> Vec<String> {
    let mut lines = Vec::new();
    let mut line = String::new();
    for word in text.split_whitespace() {
        let mut word = word.to_owned();
        if !line.is_empty() && line.chars().count() + 1 + word.chars().count() > width {
            lines.push(std::mem::take(&mut line));
        }
        while line.is_empty() && word.chars().count() > width {
            let split = word
                .char_indices()
                .nth(width)
                .map_or(word.len(), |(index, _)| index);
            let rest = word.split_off(split);
            lines.push(word);
            word = rest;
        }
        if !line.is_empty() {
            line.push(' ');
        }
        line.push_str(&word);
    }
    if !line.is_empty() {
        lines.push(line);
    }
    lines
}

fn invocation(item: &Item<'_, '_>) -> String {
    match item {
        Item::Commands(_) | Item::Pos(_) => metavar(item),
        Item::Help | Item::Opt(_) => {
            let flags = item.flags().join(", ");
            if item.takes_value() {
                format!("{flags} {}", metavar(item))
            } else {
                flags
            }
        }
    }
}

fn item_help<'x>(item: &'x Item<'_, '_>) -> Option<&'x str> {
    match item {
        Item::Help => Some(HELP_TEXT),
        Item::Opt(opt) => opt.help,
        Item::Pos(pos) => pos.help,
        Item::Commands(_) => None,
    }
}

fn format_entry(
    output: &mut String,
    indent: usize,
    header: &str,
    help: Option<&str>,
    position: usize,
) {
    let help = help.map(str::trim).filter(|help| !help.is_empty());
    let action_width = position.saturating_sub(indent + 2);
    let Some(help) = help else {
        output.push_str(&format!("{}{header}\n", " ".repeat(indent)));
        return;
    };
    let lines = wrap_words(help, WIDTH.saturating_sub(position).max(11));
    if header.chars().count() <= action_width {
        let padding = action_width - header.chars().count();
        output.push_str(&format!(
            "{}{header}{}  ",
            " ".repeat(indent),
            " ".repeat(padding)
        ));
    } else {
        output.push_str(&format!("{}{header}\n", " ".repeat(indent)));
        output.push_str(&" ".repeat(position));
    }
    for (index, line) in lines.iter().enumerate() {
        if index > 0 {
            output.push_str(&" ".repeat(position));
        }
        output.push_str(line);
        output.push('\n');
    }
}

/// `--help` output in argparse's layout.
pub fn help(parser: &Parser<'_>, prog: &str) -> String {
    let all = items(parser);
    let positionals: Vec<Item<'_, '_>> = all
        .iter()
        .copied()
        .filter(|item| item.flags().is_empty())
        .collect();
    let optionals: Vec<Item<'_, '_>> = all
        .iter()
        .copied()
        .filter(|item| !item.flags().is_empty())
        .collect();
    let mut longest = 0;
    for item in &all {
        longest = longest.max(invocation(item).chars().count() + 2);
        if let Item::Commands(commands) = item {
            for command in commands.commands.iter().filter(|c| c.help.is_some()) {
                longest = longest.max(command.name.chars().count() + 4);
            }
        }
    }
    let position = (longest + 2).min(MAX_HELP_POSITION);
    let mut output = usage(parser, prog);
    output.push('\n');
    if let Some(description) = parser.description {
        for line in wrap_words(description, WIDTH) {
            output.push_str(&line);
            output.push('\n');
        }
        output.push('\n');
    }
    for (title, section) in [
        ("positional arguments:", &positionals),
        ("options:", &optionals),
    ] {
        if section.is_empty() {
            continue;
        }
        output.push_str(title);
        output.push('\n');
        for item in section {
            format_entry(&mut output, 2, &invocation(item), item_help(item), position);
            if let Item::Commands(commands) = item {
                for command in commands.commands.iter().filter(|c| c.help.is_some()) {
                    format_entry(&mut output, 4, command.name, command.help, position);
                }
            }
        }
        output.push('\n');
    }
    while output.ends_with("\n\n") {
        output.pop();
    }
    output
}
