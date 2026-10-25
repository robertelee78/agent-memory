"""Bootstrap contract: exercise real subprocesses/files, not product internals."""
import ast
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import unittest

from world import World, require_world
from fault_partitions import OWNERS

ROOT = Path(__file__).resolve().parents[2]


class HarnessContract(unittest.TestCase):
    def test_worker_is_isolated_and_credentials_are_absent(self):
        root = require_world()
        self.assertTrue(Path.home().is_relative_to(root))
        self.assertTrue(Path.cwd().is_relative_to(root))
        for key in ("GH_TOKEN", "GITHUB_TOKEN", "SSH_AUTH_SOCK", "AWS_ACCESS_KEY_ID",
                    "OPENAI_API_KEY", "ANTHROPIC_API_KEY", "GIT_DIR", "GIT_WORK_TREE",
                    "GIT_CONFIG_COUNT", "PYTHONPATH"):
            self.assertNotIn(key, os.environ)

    def test_public_command_runs_with_spaces_and_clean_environment(self):
        with World() as world:
            script = world.inside("command with spaces.py")
            script.write_text("import json, os; print(json.dumps(dict(os.environ)))\n")
            result = world.run([sys.executable, "-I", script])
            env = json.loads(result.stdout)
            self.assertEqual(env["HOME"], str(world.paths["home"]))
            self.assertNotEqual(env["HOME"], str(Path.home()))
            for path in world.paths.values():
                self.assertTrue(path.is_relative_to(world.root))

    def test_real_git_bare_remote_clones_and_worktree(self):
        with World() as world:
            remote = world.repository("remote.git", bare=True)
            a = world.clone(remote, "clone A")
            (a / "lesson.md").write_text("Durable lesson\n")
            world.git("add", "lesson.md", cwd=a)
            world.git("commit", "-m", "Learn a lesson", cwd=a)
            world.git("push", "origin", "main", cwd=a)
            b = world.clone(remote, "clone B")
            self.assertEqual((b / "lesson.md").read_text(), "Durable lesson\n")
            worktree = world.inside("other session")
            world.git("worktree", "add", "-b", "session-two", str(worktree), cwd=b)
            self.assertEqual(world.git("branch", "--show-current", cwd=worktree).stdout.strip(), "session-two")
            # No real credential helper, system config, or network Git transport.
            denied = world.git("ls-remote", "https://example.invalid/repo", check=False)
            self.assertNotEqual(denied.returncode, 0)
            self.assertIn("transport 'https' not allowed", denied.stderr)

    def test_escape_and_live_root_overrides_are_refused(self):
        with World() as world:
            for path in ("../outside", Path.home(), "/"):
                with self.assertRaises(ValueError):
                    world.inside(path)
            (world.root / "escape").symlink_to(Path.home(), target_is_directory=True)
            with self.assertRaises(ValueError):
                world.run([sys.executable, "-V"], cwd="escape")
            with self.assertRaises(ValueError):
                world.run("echo unsafe")
            with self.assertRaises(ValueError):
                world.run([sys.executable, "-V"], env={"HOME": str(Path.home())})
            altered = world.env | {"HOME": str(Path.home())}
            with self.assertRaises(ValueError):
                require_world(altered)

    def test_command_failure_and_timeout_are_not_success(self):
        with World() as world:
            result = world.run([sys.executable, "-c", "raise SystemExit(7)"], check=False)
            self.assertEqual(result.returncode, 7)
            with self.assertRaises(subprocess.CalledProcessError):
                world.run([sys.executable, "-c", "raise SystemExit(7)"])
            with self.assertRaises(subprocess.TimeoutExpired):
                world.run([sys.executable, "-c", "import time; time.sleep(10)"], timeout=0.05)

    def test_world_cleanup_does_not_follow_escape_symlink(self):
        with World() as outer:
            sentinel = outer.inside("sentinel")
            sentinel.write_text("keep")
            with World() as inner:
                removed_root = inner.root
                (inner.root / "escape").symlink_to(sentinel)
            self.assertFalse(removed_root.exists())
            self.assertEqual(sentinel.read_text(), "keep")

    def test_worker_rejects_a_live_home_before_importing_suite(self):
        with World() as world:
            sentinel = world.inside("must-not-run")
            suite = world.inside("hostile-suite.py")
            suite.write_text(f"from pathlib import Path; Path({str(sentinel)!r}).touch()\n")
            # Invoke the real worker with an intentionally invalid root environment.
            result = subprocess.run(
                [sys.executable, "-I", str(ROOT / "scripts/run_contract_suite.py"), str(suite)],
                cwd=world.paths["repo"], env=world.env | {"HOME": "/"},
                capture_output=True, text=True, timeout=10,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("Refusing non-isolated HOME", result.stderr)
            self.assertFalse(sentinel.exists())

    def test_actual_gate_rejects_empty_skipped_and_failing_suites(self):
        # Copy the gate into a disposable checkout so corruption fixtures never
        # edit the real suite, manifest or product repository.
        samples = {
            "empty": "",
            "skipped": "import unittest\nclass Contract(unittest.TestCase):\n @unittest.skip('missing')\n def test_x(self): pass\n",
            "failure": "import unittest\nclass Contract(unittest.TestCase):\n def test_x(self): self.fail('real failure')\n",
            "passed": "import unittest\nclass Contract(unittest.TestCase):\n def test_x(self): self.assertTrue(True)\n",
        }
        # Only the disposable gate's controlled positive fixture fabricates
        # coverage metadata. These runs are gate-control tests, never product
        # interruption evidence. The real suites emit actual replay results.
        proof_fixture = '''
import json,sys
from fault_partitions import GROUPS,PARTITIONS
observed=[{'step':i,'point':'controlled-'+str(i),'kind':'controlled-write'} for i in range(PARTITIONS)]
for name,group in GROUPS.items():
 for index in range(PARTITIONS):
  proof={'group':name,'index':index,'count':PARTITIONS,'artifact_digest':'0'*64,
         'observed':observed,'executed':[observed[index]['point']]}
  print(json.dumps({'lifecycle_fault_partition':proof}),file=sys.stderr)
'''
        passed = samples['passed']
        samples['passed'] += proof_fixture
        samples['passed-without-interruption-proof'] = passed
        samples['incomplete-interruption-proof'] = passed + proof_fixture.replace(
            "'executed':[observed[index]['point']]", "'executed':[] if index==0 else [observed[index]['point']]")
        with World() as world:
            checkout = world.inside("checkout with spaces")
            (checkout / "scripts").mkdir(parents=True)
            (checkout / "tests/contracts").mkdir(parents=True)
            for name in ("test.sh", "test_gate.py", "run_contract_suite.py"):
                shutil.copy2(ROOT / "scripts" / name, checkout / "scripts" / name)
            shutil.copy2(ROOT / "tests/world.py", checkout / "tests/world.py")
            shutil.copy2(ROOT / "tests/fault_partitions.py", checkout / "tests/fault_partitions.py")
            manifest = json.loads((ROOT / "tests/contracts.json").read_text())
            # Every unrelated foundation would run the same disposable body.
            # Retain the full required contract set and every fault partition;
            # production gate runs still execute the complete real registry.
            tree = ast.parse((ROOT/'scripts/test_gate.py').read_text())
            required = ast.literal_eval(next(node.value for node in tree.body if isinstance(node,ast.Assign)
                and any(isinstance(target,ast.Name) and target.id=='REQUIRED' for target in node.targets)))
            required |= set(OWNERS) | {'harness','optional-stores','lifecycle-fault-partitions'}
            manifest['suites'] = [entry for entry in manifest['suites'] if entry['name'] in required]
            for sample, body in samples.items():
                print('gate-control-sample: '+sample, file=sys.stderr, flush=True)
                (checkout / "tests/contracts/fixture.py").write_text(body)
                for entry in manifest["suites"]:
                    if entry["scope"] in {"v0.0.1", "v0.0.2"}:
                        entry["path"] = "tests/contracts/fixture.py"
                (checkout / "tests/contracts.json").write_text(json.dumps(manifest))
                result = world.run(["sh", checkout / "scripts/test.sh", "--release", "--json"], check=False)
                report = json.loads(result.stdout)
                self.assertEqual(report["passed"], sample == "passed")
                self.assertEqual(report["release_complete"], sample == "passed")
                self.assertEqual(result.returncode == 0, sample == "passed")
            # Missing required entries and attempts to reclassify them cannot
            # make a falsely complete release report.
            manifest["suites"] = [e for e in manifest["suites"] if e["name"] != "bank-roundtrip"]
            (checkout / "tests/contracts.json").write_text(json.dumps(manifest))
            result = world.run(["sh", checkout / "scripts/test.sh", "--release", "--json"], check=False)
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("every mandatory contract", json.loads(result.stdout)["error"])


if __name__ == "__main__":
    unittest.main()
