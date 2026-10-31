#!/usr/bin/env python3
"""Source-only startup: never import installed AM bytecode as its own verifier.

This standard-library-only file is executed as source, not imported. The trusted
interpreter, standard library and executing source remain the trust base.
"""
from pathlib import Path
import os
import runpy
import stat
import sys
import tempfile


def isolate():
    # Ignore TMPDIR: an arbitrary parent could replace even a private child.
    # Both supported systems provide a root-owned sticky system temporary root.
    parent = Path('/tmp').resolve(strict=True)
    node = Path(parent.anchor)
    for part in (None, *parent.parts[1:]):
        if part is not None:
            node = node / part
        value = node.lstat()
        if (not stat.S_ISDIR(value.st_mode) or value.st_uid != 0
                or (value.st_mode & 0o022 and not value.st_mode & stat.S_ISVTX)):
            raise RuntimeError('Unsafe source-only startup temporary root; preserve the runtime and inspect system temporary-directory ownership.')
    guard = tempfile.TemporaryDirectory(prefix='am-source-only-', dir=parent)
    value = Path(guard.name).lstat()
    if (not stat.S_ISDIR(value.st_mode) or value.st_uid != os.geteuid()
            or stat.S_IMODE(value.st_mode) != 0o700):
        guard.cleanup()
        raise RuntimeError('Source-only startup requires a fresh private empty cache directory.')
    sys.dont_write_bytecode = True
    sys.pycache_prefix = guard.name
    return guard


if __name__ == '__main__':
    with isolate():
        sys.argv = sys.argv[1:]
        if not sys.argv:
            raise SystemExit('Source-only startup requires a script path.')
        if sys.argv[0] == '-c':
            if len(sys.argv) < 2:
                raise SystemExit('Source-only startup requires code after -c.')
            code = sys.argv[1]
            sys.argv = ['-c', *sys.argv[2:]]
            exec(compile(code, '<string>', 'exec'), {'__name__': '__main__'})
        elif sys.argv[0] == '-m':
            if len(sys.argv) < 2:
                raise SystemExit('Source-only startup requires a module after -m.')
            module = sys.argv[1]
            sys.argv = sys.argv[1:]
            runpy.run_module(module, run_name='__main__', alter_sys=True)
        elif sys.argv[0] == '-':
            exec(compile(sys.stdin.read(), '<stdin>', 'exec'), {'__name__': '__main__'})
        else:
            runpy.run_path(sys.argv[0], run_name='__main__')
