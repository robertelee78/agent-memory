"""Local read-only identity plumbing, without ambient Git redirection or hooks."""
import os
import re
import selectors
import subprocess
import time

from .frontmatter import ContractError

_QUERY_TIMEOUT = 10
_EXIT_POLL_INTERVAL = .05
_INTERRUPT_GRACE = .25


class _GitInfrastructureError(RuntimeError):
    """Incomplete supervision is never optional Git absence."""


def _invocation(repo, args, glob_pathspecs):
    env = {key: value for key, value in os.environ.items() if not key.startswith('GIT_')}
    env.update(GIT_NO_LAZY_FETCH='1', GIT_ALLOW_PROTOCOL='', GIT_OPTIONAL_LOCKS='0',
               GIT_TRACE='0', GIT_TRACE2='0', GIT_TRACE2_EVENT='0', GIT_TRACE2_PERF='0')
    argv = ['git', '--no-pager', '--no-replace-objects',
            '--glob-pathspecs' if glob_pathspecs else '--literal-pathspecs',
            '-c', 'core.fsmonitor=false', '-c', 'core.untrackedCache=false',
            '-C', str(repo), *args]
    return argv, env


def _spawn(repo, *args, glob_pathspecs=False):
    argv, env = _invocation(repo, args, glob_pathspecs)
    return subprocess.Popen(argv, env=env, stdin=None, stdout=subprocess.PIPE,
                            stderr=subprocess.PIPE, close_fds=True)


def _cleanup(processes, selector=None, *, close_selector=False, primary=None, interrupt_grace=None):
    """Cancel before reaping; only this invoking thread owns these processes."""
    faults = []
    grace = _INTERRUPT_GRACE if interrupt_grace is None else interrupt_grace
    deadline = None
    stop_waits = False
    if isinstance(primary, KeyboardInterrupt):
        deadline = getattr(primary, '_git_reap_deadline', None)
        if deadline is None:
            deadline = time.monotonic() + grace
            primary._git_reap_deadline = deadline
        stop_waits = getattr(primary, '_git_stop_reap_waits', False)
    cancelled = set()

    def fault(exc):
        nonlocal primary, deadline, stop_waits
        faults.append(exc)
        if isinstance(exc, KeyboardInterrupt):
            if isinstance(primary, KeyboardInterrupt):
                stop_waits = True
                primary._git_stop_reap_waits = True
            else:
                if primary is not None:
                    exc.add_note('Interrupted Git cleanup after ' + type(primary).__name__)
                primary = exc
                deadline = getattr(exc, '_git_reap_deadline', None)
                if deadline is None:
                    deadline = time.monotonic() + grace
                    exc._git_reap_deadline = deadline
                stop_waits = getattr(exc, '_git_stop_reap_waits', False)
        elif isinstance(exc, SystemExit) and not isinstance(primary, (KeyboardInterrupt, SystemExit)):
            if primary is not None:
                exc.add_note('Interrupted Git cleanup after ' + type(primary).__name__)
            primary = exc

    def finish(operation):
        # An interrupt must not abandon the remaining kills/closes. No command
        # is retried, and an interrupted reap is never resumed by this helper.
        while True:
            try:
                operation()
                return True
            except (KeyboardInterrupt, SystemExit) as exc:
                fault(exc)
            except BaseException as exc:
                fault(exc)
                return False

    for process in processes:
        if process.returncode is not None or getattr(process, '_git_cleanup_handoff', False):
            continue
        def kill(process=process):
            if process.returncode is None:
                process.kill()
        if finish(kill):
            cancelled.add(process)
    for process in processes:
        for stream in (process.stdout, process.stderr):
            if stream is None or stream.closed:
                continue
            if selector is not None:
                def unregister(stream=stream):
                    try:
                        selector.get_key(stream)
                    except KeyError:
                        return
                    selector.unregister(stream)
                finish(unregister)
            finish(stream.close)
    if close_selector and selector is not None:
        finish(selector.close)
    for process in processes:
        if process.returncode is not None or process not in cancelled or stop_waits:
            continue
        wait_started = None
        try:
            wait_started = time.monotonic()
            if deadline is None:
                process.wait()
            else:
                remaining = deadline - time.monotonic()
                if remaining > 0:
                    process.wait(timeout=remaining)
        except subprocess.TimeoutExpired as exc:
            if deadline is None:
                fault(exc)
        except BaseException as exc:
            if (isinstance(exc, KeyboardInterrupt) and deadline is None
                    and wait_started is not None and getattr(exc, '_git_reap_deadline', None) is None):
                # Popen.wait may already have spent its own interrupt grace.
                exc._git_reap_deadline = wait_started + grace
            fault(exc)
            # A KeyboardInterrupt arising in an ordinary wait switches to the
            # bounded policy for subsequent children, never an unbounded retry.
    unreaped = sum(process.returncode is None for process in processes)
    if isinstance(primary, KeyboardInterrupt):
        for process in processes:
            if process.returncode is None:
                process._git_cleanup_handoff = True
        primary.add_note(f'Git cleanup: {unreaped} killed-but-unreaped or cancellation-unproved '
                         'children handed to standard CPython cleanup; no further signals')
    if faults:
        if primary is None:
            primary = _GitInfrastructureError('Unsafe Git query cleanup; no observation accepted')
            primary.__cause__ = faults[0]
        for exc in faults:
            if exc is not primary:
                primary.add_note('Git cleanup fault: ' + type(exc).__name__ + ': ' + str(exc))
    return primary, bool(faults)


def _run(repo, *args, glob_pathspecs=False):
    process = _spawn(repo, *args, glob_pathspecs=glob_pathspecs)
    primary = None
    try:
        stdout, stderr = process.communicate(timeout=_QUERY_TIMEOUT)
        command = subprocess.CompletedProcess(process.args, process.returncode, stdout, stderr)
    except BaseException as exc:
        primary = exc
    # communicate already consumes CPython's KeyboardInterrupt grace.
    primary, unsafe = _cleanup((process,), primary=primary, interrupt_grace=0)
    if primary is not None:
        if unsafe and not isinstance(primary, (KeyboardInterrupt, SystemExit, _GitInfrastructureError)):
            raise _GitInfrastructureError('Unsafe Git query cleanup; no observation accepted') from primary
        raise primary
    return command


def _git_output(command, error=None, *, optional=False):
    if error is not None:
        if isinstance(error, (FileNotFoundError, subprocess.TimeoutExpired)):
            if optional:
                return None
            raise ContractError('Cannot inspect local repository identity evidence') from error
        raise error
    if command.returncode:
        if optional:
            return None
        raise ContractError('Cannot inspect repository evidence with Git; no identity change performed')
    # Remove the command terminator only. Spaces and newlines can be part of a path.
    return os.fsdecode(command.stdout).removesuffix('\n')


def git(repo, *args, optional=False, glob_pathspecs=False):
    try:
        command = _run(repo, *args, glob_pathspecs=glob_pathspecs)
    except (FileNotFoundError, subprocess.TimeoutExpired) as exc:
        return _git_output(None, exc, optional=optional)
    return _git_output(command, optional=optional)


class _Query:
    def __init__(self):
        self.process = None
        self.deadline = None
        self.output = ([], [])
        self.command = None
        self.error = None
        self.consumed = False

    @property
    def pending(self):
        return not self.consumed and self.command is None and self.error is None

    def pipes_closed(self):
        return self.process.stdout.closed and self.process.stderr.closed


class _git_group:
    """At most three queries, one invoking owner, ordered output acceptance."""

    def __init__(self, repo, specs):
        if not 1 <= len(specs) <= 3:
            raise ValueError('A Git query group requires one to three fixed commands')
        self.repo, self.specs = repo, specs
        self.queries = [_Query() for _ in specs]
        self.selector = None
        self.next = 0
        self.failure = None
        self.barrier = None
        self.closed = False

    def _processes(self):
        return tuple(query.process for query in self.queries if query.process is not None)

    def _fatal(self, exc):
        if self.failure is None:
            self.failure = _GitInfrastructureError('Git query supervision failed; no observation accepted')
            self.failure.__cause__ = exc
            self.barrier = next((i for i in range(self.next, len(self.queries))
                                 if self.queries[i].pending), len(self.queries))
        elif exc is not self.failure:
            self.failure.add_note('Additional Git supervision fault: ' + type(exc).__name__ + ': ' + str(exc))
        primary, _ = _cleanup(self._processes(), self.selector, close_selector=True,
                              primary=self.failure)
        self.closed = True
        if isinstance(primary, (KeyboardInterrupt, SystemExit)):
            raise primary

    def __enter__(self):
        try:
            self.selector = selectors.DefaultSelector()
            for query, (args, glob) in zip(self.queries, self.specs):
                self._sweep()
                try:
                    query.process = _spawn(self.repo, *args, glob_pathspecs=glob)
                except OSError as exc:
                    query.error = exc
                    continue
                query.deadline = time.monotonic() + _QUERY_TIMEOUT
                self._sweep()
                for index, stream in enumerate((query.process.stdout, query.process.stderr)):
                    if stream.closed:
                        continue
                    os.set_blocking(stream.fileno(), False)
                    self.selector.register(stream, selectors.EVENT_READ, (query, index))
                self._sweep()
        except (KeyboardInterrupt, SystemExit) as exc:
            primary, _ = _cleanup(self._processes(), self.selector, close_selector=True, primary=exc)
            self.closed = True
            raise primary
        except BaseException as exc:
            self._fatal(exc)
        return self

    def _sweep(self, *, cancel_expired=True):
        expired = []
        for query in self.queries:
            if not query.pending or query.process is None or query.deadline is None:
                continue
            if time.monotonic() < query.deadline:
                query.process.poll()
            if time.monotonic() >= query.deadline:
                query.error = subprocess.TimeoutExpired(
                    query.process.args, _QUERY_TIMEOUT,
                    output=b''.join(query.output[0]), stderr=b''.join(query.output[1]))
                query.output = ([], [])
                expired.append(query.process)
            elif query.pipes_closed() and query.process.returncode is not None:
                query.command = subprocess.CompletedProcess(
                    query.process.args, query.process.returncode,
                    b''.join(query.output[0]), b''.join(query.output[1]))
                query.output = ([], [])
        if expired and cancel_expired:
            primary, _ = _cleanup(expired, self.selector)
            if primary is not None:
                raise primary

    def _pump(self):
        self._sweep()
        pending = [query for query in self.queries if query.pending and query.process is not None]
        if not pending:
            return
        remaining = min(query.deadline for query in pending) - time.monotonic()
        if any(query.pipes_closed() and query.process.returncode is None for query in pending):
            remaining = min(remaining, _EXIT_POLL_INTERVAL)
        if remaining <= 0:
            self._sweep()
            return
        if self.selector.get_map():
            ready = self.selector.select(remaining)
            self._sweep()
            for key, _ in ready:
                self._sweep()
                query, index = key.data
                if not query.pending or key.fileobj.closed:
                    continue
                try:
                    data = os.read(key.fd, 32768)
                except BlockingIOError:
                    continue
                if data:
                    query.output[index].append(data)
                else:
                    self.selector.unregister(key.fileobj)
                    key.fileobj.close()
                self._sweep()
        else:
            try:
                pending[0].process.wait(timeout=min(remaining, _EXIT_POLL_INTERVAL))
            except subprocess.TimeoutExpired:
                pass
            self._sweep()

    def value(self, index, *, optional=False):
        if index != self.next or index >= len(self.queries):
            raise ValueError('Git query outputs must be consumed once in original order')
        try:
            if self.failure is None:
                self._sweep()
                while self.queries[index].pending:
                    self._pump()
        except (KeyboardInterrupt, SystemExit):
            raise
        except BaseException as exc:
            self._fatal(exc)
        if self.failure is not None and index >= self.barrier:
            raise self.failure
        query = self.queries[index]
        self.next += 1
        command, error = query.command, query.error
        query.command = query.error = None
        query.consumed = True
        # The process reference remains owned until context exit, including
        # cancelled/timeout children; no PID or descriptor escapes the group.
        return _git_output(command, error, optional=optional)

    def __exit__(self, kind, value, traceback):
        primary = value
        if not self.closed:
            try:
                # Classify deadlines on unwind, but cancellation/reaping must
                # use the primary interrupt's one shared cleanup budget.
                self._sweep(cancel_expired=primary is None)
            except (KeyboardInterrupt, SystemExit) as exc:
                if isinstance(primary, KeyboardInterrupt):
                    primary.add_note('Secondary interrupt at Git group exit')
                    primary._git_stop_reap_waits = True
                else:
                    primary = exc
            except BaseException as exc:
                if primary is None:
                    primary = _GitInfrastructureError('Git query supervision failed at exit')
                    primary.__cause__ = exc
                else:
                    primary.add_note('Git supervision failed at exit: ' + type(exc).__name__)
            if primary is None:
                primary = self.failure
                if primary is None and self.next != len(self.queries):
                    primary = _GitInfrastructureError('Incomplete ordered Git query observation')
            primary, _ = _cleanup(self._processes(), self.selector, close_selector=True, primary=primary)
            self.closed = True
        elif primary is None:
            primary = self.failure
        elif self.failure is not None and primary is not self.failure:
            primary.add_note('Git supervision also failed: ' + str(self.failure))
        self.queries.clear()
        self.selector = None
        if primary is not None and primary is not value:
            raise primary
        return False


def root_head(repo):
    """Observe a complete root and optional immutable HEAD without trimming paths."""
    try:
        command = _run(repo, 'rev-parse', '--show-toplevel', '--verify', '--quiet', 'HEAD')
    except (OSError, subprocess.TimeoutExpired):
        return None, None
    if command.returncode:
        # Only a completed positive failure can retain rev-parse's root output.
        # A killed process has not completed even if it emitted a plausible root.
        if command.returncode > 0 and command.stdout.endswith(b'\n') and command.stdout != b'\n':
            return os.fsdecode(command.stdout[:-1]), None
        return None, None
    output = os.fsdecode(command.stdout)
    root, separator, head = output[:-1].rpartition('\n') if output.endswith('\n') else ('', '', '')
    if not root or not separator or not re.fullmatch('[0-9a-f]{40}|[0-9a-f]{64}', head):
        raise ContractError('Repository root/HEAD observation is incomplete; do not infer absence')
    return root, head
