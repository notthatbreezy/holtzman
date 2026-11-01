import importlib.util
from pathlib import Path
import json
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    "retrieval_patch", Path(__file__).resolve().parents[1] / "scripts/patch-st-retrieval.py")
patch = importlib.util.module_from_spec(spec)
spec.loader.exec_module(patch)


class RetrievalTests(unittest.TestCase):
    def fixture(self, root, newline):
        (root / "package.json").write_text(json.dumps({"version": "1.19.0"}))
        for relative, (before, _) in patch.PATCHES.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(before.replace("\n", newline).encode())

    def test_patch_preserves_newlines_and_backups_and_is_idempotent(self):
        for newline in ["\n", "\r\n"]:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root, newline)
                backup = patch.apply(root, root / "backups")
                for relative, (before, after) in patch.PATCHES.items():
                    self.assertEqual((backup / relative).read_bytes(), before.replace("\n", newline).encode())
                    self.assertEqual((root / relative).read_bytes(), after.replace("\n", newline).encode())
                self.assertIsNone(patch.apply(root, root / "backups"))

    def test_unrecognized_embedding_code_prevents_partial_query_patch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, "\n")
            (root / "src/vectors/embedding.js").write_text("unknown code")
            with self.assertRaisesRegex(ValueError, "no changes made"):
                patch.apply(root, root / "backups")
            relative = "public/scripts/extensions/vectors/index.js"
            self.assertEqual((root / relative).read_text(), patch.QUERY_BEFORE)


if __name__ == "__main__":
    unittest.main()
