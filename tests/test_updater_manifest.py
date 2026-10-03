"""Regression: ARM success alone must never produce the release updater feed."""
import base64
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location('updater_manifest', Path(__file__).parents[1] / 'scripts/updater-manifest.py')
manifest = importlib.util.module_from_spec(spec)
spec.loader.exec_module(manifest)

class ManifestTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.names = ['xNAUT-macos-aarch64.app.tar.gz','xNAUT-macos-x64.app.tar.gz','xNAUT-1.29.2-windows-x64.msi','xNAUT-1.29.2-windows-x64-setup.exe']
        for name in self.names:
            (self.root / name).write_bytes(b'fixture artifact')
            (self.root / (name+'.sig')).write_text(base64.b64encode(b'untrusted comment: fixture\nAA==\ntrusted comment: fixture\nAA==').decode())

    def test_both_mac_architectures_and_windows_are_discoverable(self):
        result = manifest.build(self.root, '1.29.2', '48Nauts-Operator/xNaut')
        self.assertEqual(set(result['platforms']), {'darwin-aarch64','darwin-x86_64','windows-x86_64','windows-x86_64-msi','windows-x86_64-nsis'})
        self.assertTrue(result['platforms']['windows-x86_64']['url'].endswith('.msi'))
        self.assertTrue(result['platforms']['windows-x86_64-nsis']['url'].endswith('.exe'))

    def test_partial_build_and_empty_signatures_refuse_a_feed(self):
        for name in self.names:
            path = self.root / name
            path.unlink()
            with self.assertRaises(ValueError): manifest.build(self.root, '1.29.2', '48Nauts-Operator/xNaut')
            path.write_bytes(b'fixture artifact')
        (self.root / (self.names[2]+'.sig')).write_text('')
        with self.assertRaises(ValueError): manifest.build(self.root, '1.29.2', '48Nauts-Operator/xNaut')

if __name__ == '__main__': unittest.main()
