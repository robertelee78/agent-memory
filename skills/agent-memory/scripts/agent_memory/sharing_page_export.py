"""Export the checked pages of one source-bound inspection to private files."""
import hashlib
import os
from pathlib import Path
import stat

from . import private_workspace, recovery_frontier, sharing_frontier, sharing_holds, sharing_limits as limits
from .developer_identity import digest, encoded
from .frontmatter import ContractError

SCHEMA = 'agent-memory/semantic-sharing-page-export-v1'


def absent(path):
    try:
        path.lstat()
    except FileNotFoundError:
        return
    raise limits.PrivateOutputError('Preserve existing private output; choose new paths')


def preflight(repo, output, pages_dir):
    if not Path(output).is_absolute() or not Path(pages_dir).is_absolute():
        raise limits.PrivateOutputError('Choose new absolute private output paths')
    manifest = limits.preflight_private_output(output, repo)
    directory = limits.preflight_private_output(pages_dir, repo)
    if manifest == directory or manifest in directory.parents or directory in manifest.parents:
        raise limits.PrivateOutputError('Choose distinct, non-overlapping private outputs')
    # An export directory may not contain any of the forbidden trees either.
    pack = Path(__file__).resolve().parents[2]
    source = pack.parent.parent if pack.parent.name == 'skills' else pack
    if any(root == directory or directory in root.parents for root in (Path(repo).resolve(), source, pack)):
        raise limits.PrivateOutputError('Private export must not overlap repository or runtime')
    absent(manifest)
    absent(directory)
    try:
        for parent in (manifest.parent, directory.parent):
            fd, _ = private_workspace.checked_directory(parent)
            os.close(fd)
    except (ContractError, OSError, ValueError):
        raise limits.PrivateOutputError('Choose existing owner-only private output directories') from None
    return manifest, directory


def readback(dfd, name, size, expected):
    fd = os.open(name, os.O_RDONLY | os.O_NOFOLLOW | os.O_NONBLOCK, dir_fd=dfd)
    try:
        before = os.fstat(fd)
        if (not stat.S_ISREG(before.st_mode) or before.st_uid != os.geteuid()
                or stat.S_IMODE(before.st_mode) != 0o600 or before.st_nlink != 1
                or before.st_size != size):
            raise ContractError('Unsafe or changed private export file')
        hasher = hashlib.sha256()
        count = 0
        while True:
            chunk = os.read(fd, min(65536, size - count + 1))
            if not chunk:
                break
            count += len(chunk)
            if count > size:
                raise ContractError('Private export file changed during readback')
            hasher.update(chunk)
        after = os.fstat(fd)
        current = os.stat(name, dir_fd=dfd, follow_symlinks=False)
        stable = lambda info: (info.st_dev, info.st_ino, info.st_mode, info.st_uid,
                               info.st_nlink, info.st_size, info.st_mtime_ns, info.st_ctime_ns)
        if (count != size or hasher.hexdigest() != expected or stable(before) != stable(after)
                or stable(current) != stable(before)):
            raise ContractError('Private export readback did not match')
    finally:
        os.close(fd)


def write_checked(dfd, name, raw):
    private_workspace.write_fd(dfd, name, raw)
    readback(dfd, name, len(raw), digest(raw))


def export(repo, output, pages_dir, request=None):
    from . import semantic_sharing as semantic

    manifest_path, directory = preflight(repo, output, pages_dir)
    parent_fd, parent_info = private_workspace.checked_directory(directory.parent)
    manifest_fd = None
    directory_fd = None
    try:
        manifest_fd, manifest_parent_info = private_workspace.checked_directory(manifest_path.parent)
        # Repeat absence checks against the handles used for exclusive creation.
        for fd, name in ((parent_fd, directory.name), (manifest_fd, manifest_path.name)):
            try:
                os.stat(name, dir_fd=fd, follow_symlinks=False)
            except FileNotFoundError:
                continue
            raise limits.PrivateOutputError('Preserve existing private output; choose new paths')
        os.mkdir(directory.name, 0o700, dir_fd=parent_fd)
        os.fsync(parent_fd)
        directory_fd, directory_info = private_workspace.checked_directory(directory)
        private_workspace.check_directory(directory.parent, parent_info)
        token = recovery_frontier.observe(repo)
        snapshot, _, manifest, pages = sharing_frontier.build(repo, request)
        root, observation, binding, _, _ = snapshot
        binding_sha = digest(encoded(binding))
        if len(pages) != len(manifest['pages']):
            raise ContractError('Checked page inventory changed')
        total = 0
        for index, (page, descriptor) in enumerate(zip(pages, manifest['pages'])):
            raw = encoded(page)
            total += len(raw)
            if (total > limits.MAX_PRIVATE or len(raw) > limits.MAX_PAGE
                    or page.get('schema') != 'agent-memory/semantic-sharing-page-v2'
                    or page.get('index') != index or page.get('binding_sha256') != binding_sha
                    or descriptor['index'] != index or descriptor['bytes'] != len(raw)
                    or descriptor['sha256'] != digest(raw)):
                raise ContractError('Checked page does not match its bounded manifest descriptor')
            write_checked(directory_fd, str(index) + '.json', raw)
        os.fsync(directory_fd)
        manifest_raw = limits.private_bytes(manifest)
        manifest_sha = digest(manifest_raw)
        write_checked(manifest_fd, manifest_path.name, manifest_raw)
        private_workspace.check_directory(manifest_path.parent, manifest_parent_info)
        private_workspace.check_directory(directory, directory_info)
        private_workspace.check_directory(directory.parent, parent_info)
        semantic.final_sources(root, observation, binding)
        if digest(encoded(sharing_holds.load(root, observation['project_id']))) != manifest['known_inventory_sha256']:
            raise ContractError('Known hold inventory changed; inspect again')
        recovery_frontier.check(token, repo)
        # Recheck the entire private artifact set before the completion marker.
        expected_names = {str(index) + '.json' for index in range(len(pages))}
        with os.scandir(directory_fd) as entries:
            for entry in entries:
                if entry.name not in expected_names:
                    raise ContractError('Unexpected private export directory contents')
                expected_names.remove(entry.name)
        if expected_names:
            raise ContractError('Missing private export page')
        for descriptor in manifest['pages']:
            readback(directory_fd, str(descriptor['index']) + '.json', descriptor['bytes'], descriptor['sha256'])
        readback(manifest_fd, manifest_path.name, len(manifest_raw), manifest_sha)
        private_workspace.check_directory(manifest_path.parent, manifest_parent_info)
        private_workspace.check_directory(directory, directory_info)
        private_workspace.check_directory(directory.parent, parent_info)
        marker = encoded({'schema': SCHEMA, 'manifest_sha256': manifest['manifest_sha256'],
            'manifest_artifact_sha256': manifest_sha, 'binding_sha256': binding_sha,
            'page_count': len(pages)})
        write_checked(directory_fd, 'complete.json', marker)
        readback(manifest_fd, manifest_path.name, len(manifest_raw), manifest_sha)
        private_workspace.check_directory(manifest_path.parent, manifest_parent_info)
        private_workspace.check_directory(directory, directory_info)
        private_workspace.check_directory(directory.parent, parent_info)
        os.fsync(directory_fd)
        os.fsync(parent_fd)
        return {'schema':'agent-memory/private-review-artifact-v2', 'path':str(manifest_path),
            'sha256':manifest_sha, 'bytes':len(manifest_raw),
            'publication_verified':manifest['publication_verified'],
            'changed_components':len(manifest['changed']), 'review_pages':len(manifest['pages']),
            'selection_required':len(manifest['pending_selections']),
            'conflicts_pending':bool(manifest['known_causes'] or manifest['withheld_sources']),
            'canonical_writes_performed':manifest.get('canonical_writes_performed',False),
            'pages_dir':str(directory), 'page_export_sha256':digest(marker), 'page_count':len(pages)}
    finally:
        if directory_fd is not None:
            os.close(directory_fd)
        if manifest_fd is not None:
            os.close(manifest_fd)
        os.close(parent_fd)
