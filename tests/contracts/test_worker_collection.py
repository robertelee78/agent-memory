"""Worker completion and timeout both close a real descendant's open socket."""
import socket
import subprocess
import sys
import unittest

from world import World


class WorkerCollectionContract(unittest.TestCase):
    def test_completed_and_timed_out_worker_groups_do_not_leave_descendants(self):
        for mode in ('complete', 'timeout', 'record-failure'):
            with self.subTest(mode=mode), World() as world:
                address = world.inside('child.sock')
                marker = world.inside('child.pid')
                child = ('import os,socket,sys,time\nfrom pathlib import Path\n'
                         's=socket.socket(socket.AF_UNIX);s.connect(sys.argv[1]);s.sendall(b"ready")\n'
                         'Path(sys.argv[2]).write_text(str(os.getpid()))\ntime.sleep(30)\n')
                worker = world.inside('worker.py')
                worker.write_text('import subprocess,sys,time\nfrom pathlib import Path\n'
                    + 'p=subprocess.Popen([sys.executable,"-I","-c",' + repr(child)
                    + ',sys.argv[1],sys.argv[2]],stdout=subprocess.DEVNULL,stderr=subprocess.DEVNULL)\n'
                    + 'deadline=time.monotonic()+5\n'
                    + 'while not Path(sys.argv[2]).exists():\n'
                    + ' if time.monotonic()>deadline: raise RuntimeError("Child did not become ready")\n'
                    + ' time.sleep(0.01)\n'
                    + ('time.sleep(30)\n' if mode == 'timeout' else
                       'next(Path(sys.argv[1]).parent.glob("owned-command-*" )).joinpath("status.json").mkdir()\n'
                       if mode == 'record-failure' else ''))
                with socket.socket(socket.AF_UNIX) as server:
                    server.bind(str(address)); server.listen(1); server.settimeout(5)
                    closed = False
                    try:
                        if mode == 'timeout':
                            with self.assertRaises(subprocess.TimeoutExpired):
                                world.run([sys.executable, '-I', worker, address, marker], timeout=1, group=True)
                        elif mode == 'record-failure':
                            with self.assertRaises(OSError):
                                world.run([sys.executable, '-I', worker, address, marker], group=True)
                        else:
                            self.assertEqual(world.run([sys.executable, '-I', worker, address, marker], group=True).returncode, 0)
                        connection, _ = server.accept()
                        with connection:
                            connection.settimeout(1)
                            self.assertEqual(connection.recv(5), b'ready')
                            self.assertEqual(connection.recv(1), b'')
                            closed = True
                    finally:
                        # Retain safe cleanup even if a regression leaves the
                        # demonstrated descendant alive and the EOF check fails.
                        if not closed and marker.exists():
                            import os, signal
                            try: os.kill(int(marker.read_text()), signal.SIGKILL)
                            except ProcessLookupError: pass
