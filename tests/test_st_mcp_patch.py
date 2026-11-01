"""Deployment checks use temporary plugin fixtures, never the running ST install."""
import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

spec = importlib.util.spec_from_file_location(
    "st_patch", Path(__file__).resolve().parents[1] / "scripts/patch-st-mcp-validation.py")
patch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(patch)


class PatchTests(unittest.TestCase):
    def fixture(self, root):
        plugin = root / "plugin"
        plugin.mkdir()
        (plugin / "package.json").write_text(json.dumps({
            "name": "sillytavern-mcp-server", "version": "1.0.0"}))
        for relative, (old, _) in patch.PATCHES.items():
            target = plugin / relative
            target.parent.mkdir(parents=True)
            target.write_bytes(("// fixture\r\n" + old + "\r\n").encode())
        return plugin

    def test_apply_backups_newlines_and_idempotence(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plugin = self.fixture(root)
            originals = {relative: (plugin / relative).read_bytes() for relative in patch.PATCHES}
            backup = patch.apply(plugin, root / "backups")
            for relative, (_, new) in patch.PATCHES.items():
                self.assertEqual((backup / relative).read_bytes(), originals[relative])
                self.assertIn(new.encode(), (plugin / relative).read_bytes())
                self.assertIn(b"\r\n", (plugin / relative).read_bytes())
            self.assertIsNone(patch.apply(plugin, root / "backups"))

    def test_unknown_bundle_does_not_partially_change_source(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            plugin = self.fixture(root)
            source = plugin / "src/McpClient.ts"
            original = source.read_bytes()
            (plugin / "dist/index.js").write_text("unrecognized bundle")
            with self.assertRaisesRegex(ValueError, "no changes made"):
                patch.apply(plugin, root / "backups")
            self.assertEqual(source.read_bytes(), original)
            self.assertFalse((root / "backups").exists())


if __name__ == "__main__":
    unittest.main()
