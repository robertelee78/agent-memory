"""The restricted YAML frontmatter grammar shared by portable bank readers.

Nested values use JSON-compatible flow syntax. No general YAML loader is used.
"""
import json
import re

# JSON permits these Unicode separators literally, but line-based YAML readers
# treat them as framing. Preserve their values using JSON/YAML Unicode escapes.
LINE_ESCAPES = {0x85: "\\u0085", 0x2028: "\\u2028", 0x2029: "\\u2029"}


class ContractError(ValueError):
    pass


def unique_object(pairs):
    result = {}
    for key, value in pairs:
        if key in result:
            raise ContractError(f"Duplicate metadata key: {key}")
        result[key] = value
    return result


def scalar(value):
    value = value.strip()
    if not value:
        raise ContractError("Empty metadata value")
    if value[0] in '[{"' or value in {"true", "false", "null"} or re.fullmatch(r"-?\d+(?:\.\d+)?(?:[eE][+-]?\d+)?", value):
        try:
            return json.loads(value, object_pairs_hook=unique_object,
                              parse_constant=lambda _: (_ for _ in ()).throw(ContractError("Non-finite JSON value")))
        except (ValueError, TypeError) as exc:
            raise ContractError(f"Invalid JSON-compatible metadata value: {exc}") from exc
    if value.startswith("'"):
        if len(value) < 2 or not value.endswith("'") or re.search(r"(?<!')'(?!')", value[1:-1]):
            raise ContractError("Invalid single-quoted string")
        return value[1:-1].replace("''", "'")
    if value[0] in "!&*|>" or " #" in value or ": " in value or any(c in value for c in "{}[]\t"):
        raise ContractError("Unsupported YAML syntax; quote the scalar or use JSON flow syntax")
    return value


def read(text, *, allowed):
    if not text.startswith("---\n"):
        raise ContractError("Frontmatter must start at byte zero with --- and LF")
    end = text.find("\n---\n", 4)
    if end < 0:
        raise ContractError("Missing frontmatter terminator")
    lines = text[4:end].splitlines()
    metadata = {}
    index = 0
    while index < len(lines):
        match = re.fullmatch(r"([a-z][a-z0-9-]*):(?: (.*))?", lines[index])
        if not match:
            raise ContractError("Metadata must use top-level known keys; unsupported nesting or syntax")
        key, value = match.groups()
        if key not in allowed:
            raise ContractError(f"Unknown metadata key: {key}")
        if key in metadata:
            raise ContractError(f"Duplicate metadata key: {key}")
        index += 1
        if value is None or value == "":
            items = []
            while index < len(lines) and lines[index].startswith("  - "):
                item = scalar(lines[index][4:])
                if not isinstance(item, str):
                    raise ContractError("Block-list items must be scalar strings")
                items.append(item)
                index += 1
            if not items:
                raise ContractError("Metadata value or scalar block list required")
            metadata[key] = items
        else:
            metadata[key] = scalar(value)
    return metadata, text[end + 5:]


def write(metadata):
    return "---\n" + "".join(f"{key}: {json.dumps(value, ensure_ascii=False, allow_nan=False).translate(LINE_ESCAPES)}\n"
                                  for key, value in metadata.items()) + "---\n"
