"""Print the `general.*` key/value metadata of GGUF files as JSON (stdlib only).

Reads only the GGUF header; tensor data is never loaded. Arrays (tokenizer
vocabularies) are skipped and summarized by element type/count.
"""
import json
import struct
import sys

SCALARS = {0: "<B", 1: "<b", 2: "<H", 3: "<h", 4: "<I", 5: "<i", 6: "<f", 7: "<?", 10: "<Q", 11: "<q", 12: "<d"}


def read(handle, fmt):
    size = struct.calcsize(fmt)
    return struct.unpack(fmt, handle.read(size))[0]


def read_string(handle):
    return handle.read(read(handle, "<Q")).decode("utf-8", "replace")


def read_value(handle, kind):
    if kind in SCALARS:
        return read(handle, SCALARS[kind])
    if kind == 8:
        return read_string(handle)
    if kind == 9:
        item_kind, count = read(handle, "<I"), read(handle, "<Q")
        for _ in range(count):
            read_value(handle, item_kind)
        return {"array_type": item_kind, "count": count}
    raise ValueError("unknown GGUF value type %d" % kind)


def general(path):
    with open(path, "rb") as handle:
        if handle.read(4) != b"GGUF":
            raise ValueError("%s is not GGUF" % path)
        version, _tensors, kv_count = read(handle, "<I"), read(handle, "<Q"), read(handle, "<Q")
        values = {}
        for _ in range(kv_count):
            key = read_string(handle)
            value = read_value(handle, read(handle, "<I"))
            if key.startswith("general."):
                values[key] = value
        return {"path": path, "gguf_version": version, "kv_count": kv_count, "general": values}


print(json.dumps([general(path) for path in sys.argv[1:]], sort_keys=True))
