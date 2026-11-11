"""Owner-thread Git query lifecycle contracts, using synchronized real children."""
from concurrent.futures import ThreadPoolExecutor
from contextlib import contextmanager, nullcontext
import errno
import os
from pathlib import Path
import selectors
import socket
import subprocess
import sys
import tempfile
import threading
import unittest
from unittest.mock import patch

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / 'skills/agent-memory/scripts'))
from agent_memory import identity_git
from agent_memory.frontmatter import ContractError


# The control fd belongs only to this fixture. Production query pipes remain real
# binary Popen pipes; neither the supervisor nor its results are substituted.
_CHILD = '''import os, signal, socket, sys
control = socket.socket(fileno=int(sys.argv[1]))
control.settimeout(8)
control.sendall(b'ready\\n')
def receive():
    if control.recv(1) != b'x':
        raise RuntimeError('fixture control closed before release')
exec(sys.argv[2])
'''
_WAIT = 'receive()'


class Children:
    def __init__(self, case, bodies):
        self.case = case
        self.bodies = bodies
        self.processes = []
        self.controls = []
        self.streams = []

    def line(self, index):
        result = bytearray()
        while not result.endswith(b'\n'):
            chunk = self.controls[index].recv(1)
            if not chunk:
                raise AssertionError('child closed its synchronization pipe early')
            result.extend(chunk)
        return bytes(result)

    def release(self, index):
        self.controls[index].sendall(b'x')

    def eof(self, index):
        self.case.assertEqual(self.controls[index].recv(1), b'')

    def spawn(self, repo, *args, **kwargs):
        index = int(args[0])
        body = self.bodies[index]
        if isinstance(body, (FileNotFoundError, PermissionError)):
            with tempfile.TemporaryDirectory(prefix='identity-git-launch-') as directory:
                executable = Path(directory) / 'owned-git'
                if isinstance(body, PermissionError):
                    executable.write_text('#!/bin/sh\nexit 0\n')
                    executable.chmod(0o600)
                # Exercise the actual OS launch error, not a fabricated result.
                process = subprocess.Popen([str(executable)], stdin=None, stdout=subprocess.PIPE,
                                           stderr=subprocess.PIPE, close_fds=True)
                process.kill()
                process.communicate(timeout=8)
                self.case.fail('Fixture unexpectedly launched a missing/non-executable file')
        if isinstance(body, BaseException):
            raise body
        parent, child = socket.socketpair()
        parent.settimeout(8)
        try:
            process = subprocess.Popen(
                [sys.executable, '-I', '-B', '-c', _CHILD, str(child.fileno()), body],
                stdin=None, stdout=subprocess.PIPE, stderr=subprocess.PIPE,
                close_fds=True, pass_fds=(child.fileno(),))
        except BaseException:
            parent.close()
            raise
        finally:
            child.close()
        self.processes.append(process)
        self.controls.append(parent)
        self.streams.extend((process.stdout, process.stderr))
        self.case.assertEqual(self.line(len(self.controls) - 1), b'ready\n')
        return process

    def assert_closed(self, *, reaped=True):
        for stream in self.streams:
            self.case.assertTrue(stream.closed, 'query retained an owned pipe')
        if reaped:
            for process in self.processes:
                self.case.assertIsNotNone(process.returncode)
                with self.case.assertRaises(ChildProcessError):
                    os.waitpid(process.pid, os.WNOHANG)

    def close(self):
        # Test-owned rescue is deliberately after all lifecycle assertions. A
        # failed assertion must not itself leave a blocked child in the runner.
        for control in self.controls:
            try:
                control.sendall(b'x')
            except OSError:
                pass
            control.close()
        for process in self.processes:
            if process.returncode is None:
                process.kill()
            process.wait(timeout=8)
            process.stdout.close()
            process.stderr.close()


class Selector:
    def __init__(self, factory, *, hook=None, fail=None):
        self.real = factory()
        self.hook = hook
        self.fail = fail
        self.closed = False
        self.failed = False

    def fault(self, stage):
        if stage == self.fail and not self.failed:
            self.failed = True
            # Even FileNotFoundError here is machinery failure, not optional Git
            # executable absence. The operation, not exception class, matters.
            raise FileNotFoundError(errno.ENOENT, 'owned selector ' + stage)

    def register(self, *args, **kwargs):
        self.fault('register')
        return self.real.register(*args, **kwargs)

    def unregister(self, *args, **kwargs):
        result = self.real.unregister(*args, **kwargs)
        self.fault('unregister')
        return result

    def select(self, timeout=None):
        self.fault('select')
        if self.hook is not None:
            shortened = self.hook(self, timeout)
            if shortened is not None:
                # A virtual deadline advance must not spend the old timeout
                # again in the real selector. Readiness is still genuine.
                timeout = shortened if timeout is None else min(timeout, shortened)
        return self.real.select(timeout)

    def get_map(self):
        return self.real.get_map()

    def get_key(self, fileobj):
        return self.real.get_key(fileobj)

    def close(self):
        self.real.close()
        self.closed = True
        self.fault('close')


class ClosingPipe:
    """A real owned pipe whose close completes before reporting an OS fault."""
    def __init__(self, stream):
        self.stream = stream
        self.failed = False

    @property
    def closed(self):
        return self.stream.closed

    def fileno(self):
        return self.stream.fileno()

    def close(self):
        self.stream.close()
        if not self.failed:
            self.failed = True
            raise OSError(errno.EIO, 'owned close fault after descriptor release')


class IdentityGitGroup(unittest.TestCase):
    @contextmanager
    def children(self, *bodies, selector_hook=None, selector_fail=None):
        fleet = Children(self, bodies)
        owned_selectors = []
        factory = selectors.DefaultSelector
        def create_selector():
            selector = Selector(factory, hook=selector_hook, fail=selector_fail)
            owned_selectors.append(selector)
            return selector
        try:
            with patch.object(identity_git, '_spawn', side_effect=fleet.spawn), \
                    patch.object(identity_git.selectors, 'DefaultSelector', side_effect=create_selector):
                yield fleet
            self.assertTrue(all(selector.closed for selector in owned_selectors))
        finally:
            fleet.close()
            for selector in owned_selectors:
                if not selector.closed:
                    selector.real.close()

    def specs(self, count):
        return tuple(((str(index),), False) for index in range(count))

    def test_success_drains_both_pipes_under_pressure_and_preserves_exact_bytes(self):
        # Each child exceeds ordinary pipe capacity on both streams before the
        # next chunk. Awaiting only position zero or only stdout deadlocks here.
        body = "for _ in range(96):\n os.write(1, b'o' * 8192)\n os.write(2, b'e' * 8192)\nos.write(1, b'\\n\\n')"
        with self.children(body, body, body) as children:
            with identity_git._git_group('.', self.specs(3)) as group:
                for index in range(3):
                    self.assertEqual(group.value(index), 'o' * (96 * 8192) + '\n')
            children.assert_closed()

    def test_command_failures_are_consumed_in_original_order_at_every_position(self):
        for position in range(3):
            for outcome in ('missing', 'permission', 'fd-limit', 'nonzero', 'signal', 'timeout'):
                with self.subTest(position=position, outcome=outcome):
                    failure = {
                        'missing': FileNotFoundError('owned missing executable'),
                        'permission': PermissionError('owned executable denied'),
                        'fd-limit': OSError(errno.EMFILE, 'owned Popen descriptor exhaustion'),
                        'nonzero': 'os.write(1, b"partial"); sys.exit(7)',
                        'signal': 'os.write(1, b"partial"); os.kill(os.getpid(), signal.SIGTERM)',
                        'timeout': _WAIT,
                    }[outcome]
                    bodies = ['os.write(1, b"earlier\\n")'] * 3
                    bodies[position] = failure
                    with self.children(*bodies) as children, \
                            patch.object(identity_git, '_QUERY_TIMEOUT', .3):
                        expected = (PermissionError if outcome == 'permission' else
                                    OSError if outcome == 'fd-limit' else ContractError)
                        with self.assertRaises(expected) as caught:
                            with identity_git._git_group('.', self.specs(3)) as group:
                                for index in range(position):
                                    self.assertEqual(group.value(index), 'earlier')
                                group.value(position)
                        if outcome in ('missing', 'timeout'):
                            self.assertIn('Cannot inspect local repository identity evidence', str(caught.exception))
                            self.assertIsInstance(caught.exception.__cause__,
                                                  FileNotFoundError if outcome == 'missing' else subprocess.TimeoutExpired)
                        elif outcome in ('nonzero', 'signal'):
                            self.assertIn('Cannot inspect repository evidence with Git', str(caught.exception))
                        children.assert_closed()

    def test_optional_command_absence_does_not_hide_later_required_error(self):
        for failure in (FileNotFoundError('missing'), 'sys.exit(3)',
                        'os.kill(os.getpid(), signal.SIGTERM)', _WAIT):
            with self.subTest(failure=failure), \
                    self.children(failure, 'sys.exit(9)') as children, \
                    patch.object(identity_git, '_QUERY_TIMEOUT', .3):
                with identity_git._git_group('.', self.specs(2)) as group:
                    self.assertIsNone(group.value(0, optional=True))
                    with self.assertRaisesRegex(ContractError, 'Cannot inspect repository evidence with Git'):
                        group.value(1)
                children.assert_closed()
        with self.children(PermissionError('not absence'), _WAIT) as children:
            with self.assertRaises(PermissionError):
                with identity_git._git_group('.', self.specs(2)) as group:
                    group.value(0, optional=True)
            children.assert_closed()

    def test_later_timeout_does_not_cancel_earlier_pending_success(self):
        now = [0.0]
        advanced = []
        def advance(selector, timeout):
            if len(children.processes) == 2 and not advanced:
                # Change only child one's own timeout at its launch boundary;
                # expiring it must not manufacture child zero's failure.
                advanced.append(True)
                now[0] = 2
                children.release(0)
                children.eof(0)
        with self.children(_WAIT + '; os.write(1, b"first\\n")', _WAIT,
                           selector_hook=advance) as children:
            spawn = children.spawn
            def launch(repo, *args, **kwargs):
                process = spawn(repo, *args, **kwargs)
                identity_git._QUERY_TIMEOUT = 1 if args[0] == '1' else 10
                return process
            with patch.object(identity_git, '_spawn', side_effect=launch), \
                    patch.object(identity_git, '_QUERY_TIMEOUT', 10), \
                    patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]):
                with identity_git._git_group('.', self.specs(2)) as group:
                    self.assertEqual(group.value(0), 'first')
                    self.assertIsNone(group.value(1, optional=True))
            children.assert_closed()

    def test_b1_closed_pipe_child_is_observed_before_quiet_sibling_deadline(self):
        closed = 'os.close(1); os.close(2); control.sendall(b"closed\\n"); receive()'
        released = []
        def on_select(selector, timeout):
            first = children.processes[0]
            if first.stdout.closed and first.stderr.closed and first.returncode is None:
                self.assertIsNotNone(timeout)
                self.assertLessEqual(timeout, .05)
                if not released:
                    self.assertEqual(children.line(0), b'closed\n')
                    children.release(0)
                    children.eof(0)
                    released.append(True)
        with self.children(closed, _WAIT, selector_hook=on_select) as children:
            refusal = ContractError('cancel quiet sibling after observing first result')
            with self.assertRaises(ContractError) as caught:
                with identity_git._git_group('.', self.specs(2)) as group:
                    self.assertEqual(group.value(0), '')
                    self.assertTrue(released, 'fixture must reach EOF-before-waitable with a quiet sibling')
                    raise refusal
            self.assertIs(caught.exception, refusal)
            children.assert_closed()

    def test_selector_and_fd_infrastructure_never_become_optional_absence(self):
        for stage in ('construct', 'register', 'select', 'unregister', 'close', 'nonblocking', 'read'):
            with self.subTest(stage=stage), self.children(_WAIT, _WAIT) as children:
                factory = identity_git.selectors.DefaultSelector
                read = os.read
                fired = []
                created = []
                def construct():
                    if stage == 'construct':
                        raise FileNotFoundError('selector creation, not Git absence')
                    if stage in ('register', 'select', 'unregister', 'close'):
                        selector = Selector(selectors.SelectSelector, fail=stage)
                        created.append(selector)
                        return selector
                    return factory()
                def nonblocking(fd, blocking):
                    raise OSError(errno.EMFILE, 'owned nonblocking setup exhaustion')
                def read_failure(fd, count):
                    if fd in [stream.fileno() for stream in children.streams if not stream.closed]:
                        fired.append(True)
                        raise FileNotFoundError('owned pipe service, not Git absence')
                    return read(fd, count)
                # EOF drives unregister; a real write drives read. Other stages
                # use blocked children, proving cleanup of partial setup.
                if stage in ('unregister', 'close', 'read'):
                    children.bodies = ('os.write(1, b"real bytes\\n")', 'os.write(1, b"second\\n")')
                with patch.object(identity_git.selectors, 'DefaultSelector', side_effect=construct), \
                        (patch.object(identity_git.os, 'set_blocking', side_effect=nonblocking) if stage == 'nonblocking' else nullcontext()):
                    with (patch.object(identity_git.os, 'read', side_effect=read_failure) if stage == 'read' else nullcontext()):
                        with self.assertRaises(identity_git._GitInfrastructureError):
                            with identity_git._git_group('.', self.specs(2)) as group:
                                group.value(0, optional=True)
                                group.value(1, optional=True)
                if stage == 'read':
                    self.assertTrue(fired)
                children.assert_closed()
                self.assertTrue(all(selector.closed for selector in created))

    def test_earlier_parser_refusal_survives_later_command_and_cleanup_faults(self):
        primary = ContractError('owned malformed earlier config')
        with self.children('os.write(1, b"malformed\\n")', PermissionError('later launch')) as children:
            with self.assertRaises(ContractError) as caught:
                with identity_git._git_group('.', self.specs(2)) as group:
                    self.assertEqual(group.value(0), 'malformed')
                    raise primary
            self.assertIs(caught.exception, primary)
            children.assert_closed()
        with self.children('os.write(1, b"bad\\n")', _WAIT, selector_fail='close') as children:
            with self.assertRaises(ContractError) as caught:
                with identity_git._git_group('.', self.specs(2)) as group:
                    group.value(0)
                    raise primary
            self.assertIs(caught.exception, primary)
            self.assertTrue(getattr(caught.exception, '__notes__', ()) or caught.exception.__context__)
            children.assert_closed()

    def test_descendant_held_pipes_timeout_without_waiting_for_descendant_eof(self):
        body = '''pid = os.fork()
if pid:
    os._exit(0)
control.sendall(b'descendant\\n')
receive()
control.sendall(b'released\\n')
'''
        with self.children(body) as children, patch.object(identity_git, '_QUERY_TIMEOUT', .3):
            with identity_git._git_group('.', self.specs(1)) as group:
                self.assertIsNone(group.value(0, optional=True))
            children.assert_closed()
            # The descendant is still alive and holding both write ends. Only
            # the fixture releases it; production must never signal it.
            self.assertEqual(children.line(0), b'descendant\n')
            children.release(0)
            self.assertEqual(children.line(0), b'released\n')
            children.eof(0)

    def test_each_deadline_starts_at_own_popen_return_not_first_launch(self):
        now = [0.0]
        def on_select(selector, timeout):
            if now[0] == 9:
                now[0] = 11
                children.release(1)
                children.eof(1)
        with self.children(_WAIT, _WAIT + '; os.write(1, b"later\\n")', selector_hook=on_select) as children:
            spawn = children.spawn
            def stagger(repo, *args, **kwargs):
                if args[0] == '1':
                    now[0] = 9
                return spawn(repo, *args, **kwargs)
            with patch.object(identity_git, '_spawn', side_effect=stagger), \
                    patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]):
                with identity_git._git_group('.', self.specs(2)) as group:
                    self.assertIsNone(group.value(0, optional=True))
                    self.assertEqual(group.value(1), 'later')
            children.assert_closed()

    def test_owner_parsing_time_is_charged_at_value_and_normal_exit(self):
        for boundary in ('value', 'exit'):
            with self.subTest(boundary=boundary), self.children('os.write(1, b"config\\n")', _WAIT) as children:
                now = [0.0]
                expected = self.assertRaises(identity_git._GitInfrastructureError) if boundary == 'exit' else nullcontext()
                with patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]), expected:
                    with identity_git._git_group('.', self.specs(2)) as group:
                        self.assertEqual(group.value(0), 'config')
                        # Deterministic parser work, not a sleep or child clock.
                        now[0] = 11
                        if boundary == 'value':
                            self.assertIsNone(group.value(1, optional=True))
                children.assert_closed()
                self.assertLess(children.processes[1].returncode, 0)

    def test_already_observed_completion_survives_late_consumption(self):
        now = [0.0]
        later_ready = []
        released = []
        def on_select(selector, timeout):
            if not later_ready:
                children.eof(1)
                later_ready.append(True)
        # Position zero waits until the owner has observed position one's EOF
        # and exit. Repeated selector readiness, not sleeps, drives that state.
        with self.children(_WAIT + '; os.write(1, b"first\\n")', 'os.write(1, b"complete\\n")',
                           selector_hook=on_select) as children:
            original_poll = subprocess.Popen.poll
            def poll(process):
                result = original_poll(process)
                if (len(children.processes) > 1 and process is children.processes[1]
                        and result is not None and process.stdout.closed and process.stderr.closed
                        and not released):
                    children.release(0)
                    released.append(True)
                return result
            with patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]), \
                    patch.object(subprocess.Popen, 'poll', poll):
                with identity_git._git_group('.', self.specs(2)) as group:
                    self.assertEqual(group.value(0), 'first')
                    now[0] = 100
                    self.assertEqual(group.value(1), 'complete')
            children.assert_closed()

    def test_interrupts_during_launch_select_and_parsing_preserve_primary_and_cleanup(self):
        for stage in ('launch', 'select', 'parse'):
            for exception_type in (KeyboardInterrupt, SystemExit):
                with self.subTest(stage=stage, exception=exception_type):
                    primary = exception_type('original query interruption')
                    def interrupt(selector, timeout):
                        raise primary
                    with self.children(_WAIT, _WAIT, _WAIT,
                                       selector_hook=interrupt if stage == 'select' else None) as children:
                        spawn = children.spawn
                        def launch(repo, *args, **kwargs):
                            if stage == 'launch' and args[0] == '1':
                                # No unreturned child is claimed as owned.
                                raise primary
                            return spawn(repo, *args, **kwargs)
                        with patch.object(identity_git, '_spawn', side_effect=launch):
                            with self.assertRaises(exception_type) as caught:
                                with identity_git._git_group('.', self.specs(3)) as group:
                                    if stage == 'parse':
                                        raise primary
                                    group.value(0)
                        self.assertIs(caught.exception, primary)
                        children.assert_closed()

    def test_b2_interrupt_reap_grace_is_shared_and_reports_unreaped_count(self):
        primary = KeyboardInterrupt('original interrupt')
        now = [0.0]
        timeouts = []
        with self.children(_WAIT, _WAIT, _WAIT) as children:
            def wait(process, timeout=None):
                self.assertIsNotNone(timeout, 'interrupt cleanup must never wait unboundedly')
                self.assertLessEqual(timeout, .25 - now[0] + 1e-9)
                children.assert_closed(reaped=False)
                timeouts.append(timeout)
                now[0] += timeout
                raise subprocess.TimeoutExpired(process.args, timeout)
            # Retain real killed children without observing their exits,
            # modeling OS-delayed reap; the fault spans context teardown.
            with patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]), \
                    patch.object(subprocess.Popen, 'wait', wait):
                with self.assertRaises(KeyboardInterrupt) as caught:
                    with identity_git._git_group('.', self.specs(3)):
                        raise primary
                self.assertIs(caught.exception, primary)
            self.assertTrue(timeouts)
            self.assertLessEqual(sum(timeouts), .25 + 1e-9)
            self.assertTrue(any('3' in note and 'unreaped' in note.lower()
                                for note in getattr(primary, '__notes__', ())), primary)
            children.assert_closed(reaped=False)

    def test_b2_second_interrupt_stops_waits_but_finishes_kill_and_fd_cleanup(self):
        primary = KeyboardInterrupt('first interrupt')
        secondary = KeyboardInterrupt('second interrupt')
        waits = []
        with self.children(_WAIT, _WAIT, _WAIT) as children:
            real_kill = subprocess.Popen.kill
            killed = set()
            interrupted = []
            def kill(process):
                real_kill(process)
                killed.add(process.pid)
                if not interrupted:
                    interrupted.append(True)
                    raise secondary
            def wait(process, timeout=None):
                waits.append(process.pid)
                raise AssertionError('second interrupt must prohibit another reap wait')
            with patch.object(subprocess.Popen, 'kill', kill), \
                    patch.object(subprocess.Popen, 'wait', wait):
                with self.assertRaises(KeyboardInterrupt) as caught:
                    with identity_git._git_group('.', self.specs(3)):
                        raise primary
            self.assertIs(caught.exception, primary)
            self.assertFalse(waits)
            self.assertEqual(killed, {process.pid for process in children.processes})
            children.assert_closed(reaped=False)
            self.assertTrue(any('unreaped' in note.lower() for note in getattr(primary, '__notes__', ())))

    def test_ordinary_and_system_exit_keep_unbounded_os_wait_parity(self):
        for primary in (ContractError('ordinary refusal'), SystemExit('fatal exit')):
            with self.subTest(exception=type(primary)), self.children(_WAIT, _WAIT) as children:
                original_wait = subprocess.Popen.wait
                waited = []
                def wait(process, timeout=None):
                    waited.append(timeout)
                    return original_wait(process, timeout=timeout)
                with patch.object(subprocess.Popen, 'wait', wait):
                    with self.assertRaises(type(primary)) as caught:
                        with identity_git._git_group('.', self.specs(2)):
                            raise primary
                self.assertIs(caught.exception, primary)
                self.assertTrue(waited)
                self.assertTrue(all(timeout is None for timeout in waited))
                children.assert_closed()

    def test_b2_second_interrupt_during_reap_ends_all_remaining_waits(self):
        primary = KeyboardInterrupt('first interruption')
        secondary = KeyboardInterrupt('second interruption during reap')
        with self.children(_WAIT, _WAIT, _WAIT) as children:
            waited = []
            def wait(process, timeout=None):
                self.assertIsNotNone(timeout)
                self.assertLessEqual(timeout, .25)
                children.assert_closed(reaped=False)
                waited.append(process.pid)
                if len(waited) > 1:
                    self.fail('A second KeyboardInterrupt resumed reap waiting')
                raise secondary
            with patch.object(subprocess.Popen, 'wait', wait):
                with self.assertRaises(KeyboardInterrupt) as caught:
                    with identity_git._git_group('.', self.specs(3)):
                        raise primary
            self.assertIs(caught.exception, primary)
            self.assertEqual(len(waited), 1)
            children.assert_closed(reaped=False)
            self.assertTrue(any('3' in note and 'unreaped' in note.lower()
                                for note in getattr(primary, '__notes__', ())))

    def test_interrupt_during_ordinary_or_system_exit_reap_switches_to_shared_grace(self):
        for refusal in (ContractError('ordinary refusal'), SystemExit('original SystemExit')):
            with self.subTest(refusal=type(refusal)), self.children(_WAIT, _WAIT, _WAIT) as children:
                interrupt = KeyboardInterrupt('interrupt during ordinary reap')
                now = [0.0]
                waited = []
                def wait(process, timeout=None):
                    waited.append(timeout)
                    if len(waited) == 1:
                        self.assertIsNone(timeout)
                        # Model grace already consumed inside Popen.wait.
                        now[0] += .20
                        raise interrupt
                    self.assertIsNotNone(timeout)
                    self.assertLessEqual(timeout, .25 - now[0] + 1e-9)
                    now[0] += timeout
                    raise subprocess.TimeoutExpired(process.args, timeout)
                with patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]), \
                        patch.object(subprocess.Popen, 'wait', wait):
                    with self.assertRaises(KeyboardInterrupt) as caught:
                        with identity_git._git_group('.', self.specs(3)):
                            raise refusal
                self.assertIs(caught.exception, interrupt)
                self.assertEqual(len(waited), 2)
                self.assertLessEqual(sum(value for value in waited if value is not None), .25)
                children.assert_closed(reaped=False)

    def test_cleanup_faults_attempt_remaining_real_kills_closes_and_reaps(self):
        for stage in ('kill', 'close', 'wait'):
            for has_primary in (False, True):
                with self.subTest(stage=stage, has_primary=has_primary), \
                        self.children(_WAIT, _WAIT, _WAIT) as children:
                    primary = ContractError('selected parser refusal') if has_primary else None
                    real_kill = subprocess.Popen.kill
                    real_wait = subprocess.Popen.wait
                    killed = set()
                    waited = set()
                    failed = []
                    def kill(process):
                        real_kill(process)
                        killed.add(process.pid)
                        if stage == 'kill' and not failed:
                            failed.append(True)
                            raise OSError(errno.EIO, 'owned kill fault after signal')
                    def wait(process, timeout=None):
                        result = real_wait(process, timeout=timeout)
                        waited.add(process.pid)
                        if stage == 'wait' and not failed:
                            failed.append(True)
                            raise OSError(errno.EIO, 'owned reap fault after wait')
                        return result
                    spawn = children.spawn
                    def launch(repo, *args, **kwargs):
                        process = spawn(repo, *args, **kwargs)
                        if stage == 'close' and args[0] == '0':
                            process.stdout = ClosingPipe(process.stdout)
                            children.streams[-2] = process.stdout
                        return process
                    with patch.object(identity_git, '_spawn', side_effect=launch), \
                            patch.object(subprocess.Popen, 'kill', kill), \
                            patch.object(subprocess.Popen, 'wait', wait):
                        expected = ContractError if primary is not None else identity_git._GitInfrastructureError
                        with self.assertRaises(expected) as caught:
                            with identity_git._git_group('.', self.specs(3)):
                                if primary is not None:
                                    raise primary
                                # An incomplete observation still cleans all resources.
                    if primary is not None:
                        self.assertIs(caught.exception, primary)
                        self.assertTrue(getattr(primary, '__notes__', ()))
                    self.assertEqual(killed, {process.pid for process in children.processes})
                    self.assertTrue(all(process.pid in waited for process in children.processes[1:]))
                    children.assert_closed(reaped=stage != 'kill')

    def test_late_observed_exit_is_not_accepted_after_owner_parser_deadline(self):
        now = [0.0]
        late = 'os.close(1); os.close(2); control.sendall(b"closed\\n"); receive()'
        with self.children('os.write(1, b"config\\n")', late) as children:
            with patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]):
                with identity_git._git_group('.', self.specs(2)) as group:
                    self.assertEqual(group.value(0), 'config')
                    self.assertEqual(children.line(1), b'closed\n')
                    now[0] = 11
                    children.release(1)
                    children.eof(1)
                    self.assertIsNone(group.value(1, optional=True))
            children.assert_closed()

    def test_sequential_runner_keeps_real_partial_timeout_diagnostics_and_cleanup(self):
        with self.children('os.write(1, b"partial-out"); os.write(2, b"partial-err"); receive()') as children, \
                patch.object(identity_git, '_QUERY_TIMEOUT', .3):
            with self.assertRaises(subprocess.TimeoutExpired) as caught:
                identity_git._run('.', '0')
            self.assertEqual(caught.exception.output, b'partial-out')
            self.assertEqual(caught.exception.stderr, b'partial-err')
            children.assert_closed()
        with self.children('os.write(1, b"raw\\n\\n"); os.write(2, b"diagnostic"); sys.exit(4)') as children:
            result = identity_git._run('.', '0')
            self.assertEqual((result.returncode, result.stdout, result.stderr), (4, b'raw\n\n', b'diagnostic'))
            children.assert_closed()

    def test_b1_no_readable_fds_uses_bounded_owner_wait_then_observes_exit(self):
        body = 'os.close(1); os.close(2); control.sendall(b"closed\\n"); receive()'
        with self.children(body) as children:
            real_wait = subprocess.Popen.wait
            waited = []
            def wait(process, timeout=None):
                self.assertTrue(process.stdout.closed and process.stderr.closed)
                self.assertIsNotNone(timeout)
                self.assertLessEqual(timeout, .05)
                if not waited:
                    self.assertEqual(children.line(0), b'closed\n')
                    children.release(0)
                    children.eof(0)
                waited.append(timeout)
                return real_wait(process, timeout=timeout)
            with patch.object(subprocess.Popen, 'wait', wait):
                with identity_git._git_group('.', self.specs(1)) as group:
                    self.assertEqual(group.value(0), '')
            self.assertTrue(waited)
            children.assert_closed()

    def test_b2_interrupt_in_timeout_cleanup_never_restarts_grace_or_signals_handoff(self):
        interrupt = KeyboardInterrupt('interrupt during timeout reap')
        now = [0.0]
        def expire(selector, timeout):
            now[0] = 11
            return 0
        with self.children(_WAIT, _WAIT, _WAIT, selector_hook=expire) as children:
            real_kill = subprocess.Popen.kill
            killed = []
            waits = []
            def kill(process):
                self.assertNotIn(process.pid, killed, 'cleanup signalled a handed-off child again')
                real_kill(process)
                killed.append(process.pid)
            def wait(process, timeout=None):
                waits.append(timeout)
                if len(waits) == 1:
                    self.assertIsNone(timeout)
                    raise interrupt
                self.assertIsNotNone(timeout)
                self.assertLessEqual(timeout, 11.25 - now[0] + 1e-9)
                now[0] += timeout
                raise subprocess.TimeoutExpired(process.args, timeout)
            with patch.object(identity_git.time, 'monotonic', side_effect=lambda: now[0]), \
                    patch.object(subprocess.Popen, 'kill', kill), \
                    patch.object(subprocess.Popen, 'wait', wait):
                with self.assertRaises(KeyboardInterrupt) as caught:
                    with identity_git._git_group('.', self.specs(3)) as group:
                        group.value(0, optional=True)
            self.assertIs(caught.exception, interrupt)
            self.assertEqual(len(waits), 2)
            self.assertLessEqual(sum(value for value in waits if value is not None), .25)
            self.assertEqual(set(killed), {process.pid for process in children.processes})
            children.assert_closed(reaped=False)
            self.assertTrue(any('3' in note and 'unreaped' in note.lower()
                                for note in getattr(interrupt, '__notes__', ())))

    def test_partial_registration_failure_closes_previously_owned_children_and_selector(self):
        failed = []
        class PartialSelector(Selector):
            def register(self, *args, **kwargs):
                if len(self.real.get_map()) == 3:
                    failed.append(True)
                    raise OSError(errno.ENFILE, 'owned partial registration exhaustion')
                return super().register(*args, **kwargs)
        with self.children(_WAIT, _WAIT, _WAIT) as children:
            selector = PartialSelector(selectors.SelectSelector)
            with patch.object(identity_git.selectors, 'DefaultSelector', return_value=selector):
                with self.assertRaises(identity_git._GitInfrastructureError):
                    with identity_git._git_group('.', self.specs(3)) as group:
                        group.value(0, optional=True)
            self.assertTrue(failed)
            self.assertTrue(selector.closed)
            children.assert_closed()

    def test_machinery_memory_and_runtime_errors_are_fatal_not_optional_absence(self):
        for failure in (MemoryError('owned buffer/resource failure'), RuntimeError('owned selector failure')):
            with self.subTest(failure=type(failure)):
                def fail(selector, timeout):
                    raise failure
                with self.children(_WAIT, _WAIT, selector_hook=fail) as children:
                    with self.assertRaises(identity_git._GitInfrastructureError) as caught:
                        with identity_git._git_group('.', self.specs(2)) as group:
                            group.value(0, optional=True)
                    self.assertIs(caught.exception.__cause__, failure)
                    children.assert_closed()

    def test_terminal_earlier_command_precedence_and_optional_progress_to_fatal_barrier(self):
        for optional in (False, True):
            with self.subTest(optional=optional), \
                    self.children(FileNotFoundError('owned missing first executable'), _WAIT,
                                  selector_fail='register') as children:
                expected = identity_git._GitInfrastructureError if optional else ContractError
                with self.assertRaises(expected) as caught:
                    with identity_git._git_group('.', self.specs(2)) as group:
                        if optional:
                            self.assertIsNone(group.value(0, optional=True))
                            group.value(1, optional=True)
                        else:
                            group.value(0)
                if not optional:
                    self.assertNotIsInstance(caught.exception, identity_git._GitInfrastructureError)
                    self.assertIsInstance(caught.exception.__cause__, FileNotFoundError)
                    self.assertTrue(getattr(caught.exception, '__notes__', ()))
                children.assert_closed()

    def test_four_invoking_workers_each_finish_their_own_real_children(self):
        def observe(index):
            # A separate fleet is deliberately scoped to each invoking worker;
            # only launch substitution is shared, dispatched by thread-local.
            fleet = Children(self, ['os.write(1, b"owned\\n")'] * 3)
            local.fleet = fleet
            try:
                with identity_git._git_group('.', self.specs(3)) as group:
                    self.assertEqual([group.value(i) for i in range(3)], ['owned'] * 3)
                fleet.assert_closed()
            finally:
                fleet.close()
        local = threading.local()
        def spawn(repo, *args, **kwargs):
            return local.fleet.spawn(repo, *args, **kwargs)
        with patch.object(identity_git, '_spawn', side_effect=spawn):
            with ThreadPoolExecutor(max_workers=4) as workers:
                list(workers.map(observe, range(4)))


if __name__ == '__main__':
    unittest.main()
