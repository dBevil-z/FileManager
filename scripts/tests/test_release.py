import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('release', Path(__file__).resolve().parents[1] / 'release.py')
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class ReleaseTests(unittest.TestCase):
    def test_requires_an_exact_stable_tag(self):
        self.assertEqual(release.version_for('v0.2.4'), '0.2.4')
        for tag in ['main', '0.2.4', 'v0.2.4-beta', 'v00.2.4', 'v0.2.4;echo injected', 'v0.2.4/else']:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                release.version_for(tag)

    def test_all_update_types_use_their_own_immutable_official_asset(self):
        data = release.make_manifest('v0.2.4', dict(user='user-sig', deb='deb-sig', appimage='image-sig'), 'Notes')
        for kind, name in release.names('0.2.4').items():
            target = data['platforms']['linux-x86_64-' + kind]
            self.assertEqual(target['url'], release.REPOSITORY + '/releases/download/v0.2.4/' + name)
        self.assertEqual(data['platforms']['linux-x86_64-user']['signature'], 'user-sig')
        self.assertEqual(data['platforms']['linux-x86_64'], data['platforms']['linux-x86_64-appimage'])

    def test_missing_signature_is_never_published(self):
        with self.assertRaises(ValueError):
            release.make_manifest('v0.2.4', dict(user='', deb='sig', appimage='sig'), '')

    def test_version_mismatch_aborts_before_publication(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            (root / 'package.json').write_text('{"version":"0.2.3"}')
            with self.assertRaises(ValueError):
                release.validate_versions('v0.2.4', root)

    def fixture(self, root):
        for name in release.names('0.2.4').values():
            (root / name).write_bytes(b'demonstration installer')
            (root / (name + '.sig')).write_text('example-signature')
        data = release.make_manifest('v0.2.4', dict(user='example-signature', deb='example-signature', appimage='example-signature'), 'Notes')
        (root / 'latest.json').write_text(json.dumps(data))
        self.checksums(root)

    def checksums(self, root):
        (root / 'SHA256SUMS').write_text(''.join(f'{release.digest(p)}  {p.name}\n' for p in sorted(root.iterdir()) if p.name != 'SHA256SUMS'))

    def test_missing_or_tampered_installer_fails_verification(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); self.fixture(root)
            release.verify('v0.2.4', root)
            (root / release.names('0.2.4')['user']).write_bytes(b'changed')
            with self.assertRaises(ValueError): release.verify('v0.2.4', root)

    def test_manifest_cannot_swap_package_targets_even_with_valid_checksums(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory); self.fixture(root)
            path = root / 'latest.json'; data = json.loads(path.read_text())
            data['platforms']['linux-x86_64-user'] = data['platforms']['linux-x86_64-appimage']
            path.write_text(json.dumps(data)); self.checksums(root)
            with self.assertRaises(ValueError): release.verify('v0.2.4', root)


if __name__ == '__main__': unittest.main()
