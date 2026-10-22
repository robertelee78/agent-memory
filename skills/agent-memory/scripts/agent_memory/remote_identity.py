"""Sanitized repository identifiers, never URLs to fetch or push."""
from pathlib import Path
import re
from urllib.parse import unquote, urlsplit

from .frontmatter import ContractError


def normalize(value, base=None):
    # Never include the raw input in errors: URLs may contain credentials.
    if not isinstance(value, str) or not value or len(value) > 4096 or any(ord(c) < 32 for c in value) or "\\" in value:
        raise ContractError("Invalid repository remote; inspect configuration privately")
    if re.match(r"^[A-Za-z][A-Za-z0-9+-]*::", value):
        raise ContractError("External remote helpers require an explicit supported repository binding")
    if "://" not in value:
        scp = re.fullmatch(r"(?:[^/@:]+@)?(\[[^\]]+\]|[^/:]+):(.+)", value)
        if scp:
            value = "ssh://" + scp[1] + "/" + scp[2].lstrip("/")
        else:
            value = (Path(base or Path.cwd()) / value).resolve().as_uri()
    try:
        parts = urlsplit(value)
        if parts.scheme == "file":
            if parts.netloc not in {"", "localhost"} or not parts.path.startswith("/"):
                raise ValueError("nonlocal file URL")
            path = Path(unquote(parts.path)).resolve().as_posix().rstrip("/")
            if any(ord(c) < 32 or ord(c) == 127 for c in path):
                raise ValueError("invalid local path")
            path = path.removesuffix(".git")
            if not path:
                raise ValueError("empty repository path")
            return "file://" + path
        if parts.scheme not in {"https", "http", "ssh", "git", "repo"} or not parts.hostname:
            raise ValueError("unsupported remote syntax")
        host, port = parts.hostname.lower(), parts.port
        if parts.scheme != "repo" and port == {"https": 443, "http": 80, "ssh": 22, "git": 9418}.get(parts.scheme):
            port = None
        host = "[" + host + "]" if ":" in host else host
        authority = host + (":" + str(port) if port is not None else "")
        path = parts.path.strip("/")
        # Decode only unreserved characters. Encoded separators are ambiguous.
        def escape(match):
            char = chr(int(match[1], 16))
            if char in "/\\" or ord(char) < 32:
                raise ValueError("ambiguous encoded path")
            return char if re.fullmatch(r"[A-Za-z0-9._~-]", char) else "%" + match[1].upper()
        if re.search(r"%(?![0-9a-fA-F]{2})", path):
            raise ValueError("bad escape")
        path = re.sub(r"%([0-9a-fA-F]{2})", escape, path)
        # Only this public service's documented case rule is known here.
        if parts.hostname.lower() == "github.com":
            path = path.lower()
        path = path.removesuffix(".git")
        if not path or any(p in {"", ".", ".."} for p in path.split("/")) or any(c.isspace() for c in path):
            raise ValueError("ambiguous repository path")
        return "repo://" + authority + "/" + path
    except (ValueError, UnicodeError) as exc:
        raise ContractError("Unsupported or ambiguous repository remote; choose an explicit canonical binding") from exc
