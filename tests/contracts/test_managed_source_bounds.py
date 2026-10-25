"""Actual descriptor/process boundaries for the legacy ownership verifier."""
import os
from pathlib import Path
import subprocess
import sys
import unittest
from unittest.mock import patch

from world import World

sys.path.insert(0, str(Path(__file__).resolve().parents[2]/'skills/agent-memory/scripts'))
from agent_memory import managed_source
from agent_memory.frontmatter import ContractError


class ManagedSourceBoundsContract(unittest.TestCase):
    def test_nonportable_executable_cannot_lose_owner_execution_but_keep_group_execution(self):
        with World() as world:
            root = world.repository('mode-check')
            node = root/'runner.sh'; node.write_text('#!/bin/sh\nexit 0\n'); node.chmod(0o755)
            world.git('add', '.', cwd=root); world.git('commit', '-m', 'Executable source', cwd=root)
            revision = world.git('rev-parse', 'HEAD', cwd=root).stdout.strip()
            managed_source.verify_payload(root, revision)
            node.chmod(0o654)
            with self.assertRaisesRegex(ContractError, 'executable intent differ'):
                managed_source.verify_payload(root, revision)

    def test_replacement_refs_cannot_substitute_another_source_tree(self):
        with World() as world:
            root = world.repository('replace-check')
            node = root/'source.txt'; node.write_text('Original source.\n')
            world.git('add', '.', cwd=root); world.git('commit', '-m', 'Original source', cwd=root)
            original = world.git('rev-parse', 'HEAD', cwd=root).stdout.strip()
            node.write_text('Substituted source.\n')
            world.git('add', '.', cwd=root); world.git('commit', '-m', 'Substituted source', cwd=root)
            substitute = world.git('rev-parse', 'HEAD', cwd=root).stdout.strip()
            world.git('replace', original, substitute, cwd=root)
            with self.assertRaisesRegex(ContractError, 'source bytes or executable intent differ'):
                managed_source.verify_payload(root, original)

    def test_file_replacement_by_link_or_fifo_refuses_without_following_or_blocking(self):
        for kind in ('symlink', 'fifo'):
            with self.subTest(kind=kind), World() as world:
                node = world.inside('source'); node.write_bytes(b'original')
                observed = node.lstat(); node.unlink()
                if kind == 'fifo': os.mkfifo(node)
                else: node.symlink_to(world.inside('unrelated-missing-path'))
                with self.assertRaises((OSError, ContractError)):
                    managed_source.blob_identity(node, observed, 'sha1')

    def test_growth_during_actual_descriptor_read_is_capped(self):
        with World() as world:
            node = world.inside('source'); node.write_bytes(b'original')
            observed = node.lstat(); original_read = os.read; requested = []
            def grow(fd, count):
                if not requested:
                    with node.open('ab') as stream: stream.write(b'x' * 100)
                requested.append(count)
                return original_read(fd, count)
            with patch.object(managed_source, 'MAX_FILE', 16), patch.object(managed_source.os, 'read', grow):
                with self.assertRaisesRegex(ContractError, 'grew beyond'):
                    managed_source.blob_identity(node, observed, 'sha1')
            self.assertEqual(requested, [17])

    def test_actual_git_stdout_overflow_is_bounded_and_owned_child_is_collected(self):
        with World() as world:
            directory = world.inside('fake-bin'); directory.mkdir()
            git = directory/'git'
            git.write_text('#!'+sys.executable+'\nimport os,time\nos.write(1,b"x"*10000)\ntime.sleep(30)\n')
            git.chmod(0o700)
            real_popen = subprocess.Popen; children = []
            def spawn(*args, **kwargs):
                child = real_popen(*args, **kwargs); children.append(child); return child
            with patch.dict(os.environ, {'PATH':str(directory)}), patch.object(managed_source.subprocess, 'Popen', spawn):
                with self.assertRaisesRegex(ContractError, 'exceeded its bound'):
                    managed_source.git(world.paths['repo'], 'cat-file', 'commit', 'x', binary=True, maximum=32)
            self.assertEqual(len(children), 1); self.assertIsNotNone(children[0].poll())
