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
        for relative, pairs in patch.PATCH_GROUPS.items():
            path = root / relative
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes('\n// fixture\n'.join(before for before, _ in pairs).replace("\n", newline).encode())

    def test_patch_preserves_newlines_and_backups_and_is_idempotent(self):
        for newline in ["\n", "\r\n"]:
            with tempfile.TemporaryDirectory() as directory:
                root = Path(directory)
                self.fixture(root, newline)
                backup = patch.apply(root, root / "backups")
                for relative, pairs in patch.PATCH_GROUPS.items():
                    before = '\n// fixture\n'.join(before for before, _ in pairs)
                    after = '\n// fixture\n'.join(after for _, after in pairs)
                    self.assertEqual((backup / relative).read_bytes(), before.replace("\n", newline).encode())
                    self.assertEqual((root / relative).read_bytes(), after.replace("\n", newline).encode())
                self.assertIsNone(patch.apply(root, root / "backups"))

    def test_unrecognized_embedding_code_prevents_partial_query_patch(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, "\n")
            relative = "public/scripts/extensions/vectors/index.js"
            original = (root / relative).read_bytes()
            (root / "src/vectors/embedding.js").write_text("unknown code")
            with self.assertRaisesRegex(ValueError, "no changes made"):
                patch.apply(root, root / "backups")
            self.assertEqual((root / relative).read_bytes(), original)

    def test_existing_query_fix_upgrades_without_overwriting_other_vector_changes(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            self.fixture(root, '\n')
            path = root / 'public/scripts/extensions/vectors/index.js'
            path.write_text(path.read_text().replace(patch.QUERY_BEFORE, patch.QUERY_AFTER))
            patch.apply(root, root / 'backups')
            for _, after in patch.PATCH_GROUPS['public/scripts/extensions/vectors/index.js']:
                self.assertIn(after, path.read_text())
            self.assertIsNone(patch.apply(root, root / 'backups'))


if __name__ == "__main__":
    unittest.main()
