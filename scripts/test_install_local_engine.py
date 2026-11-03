"""Fixture-only tests for the local ezTrans installer (no vendor files)."""

import io
import tempfile
import unittest
from pathlib import Path
from zipfile import ZipFile

from install_local_engine import install, members_to_install


class BundleTests(unittest.TestCase):
    def test_installs_only_engine_assets(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = root / "input.zip"
            with ZipFile(archive, "w") as z:
                z.writestr("J2KEngineH.dll", b"dummy32")
                z.writestr("J2KEngine.dlx", b"dependency")
                z.writestr("Dat/RuleKJ.Dat", b"rules")
                z.writestr("Ehnd/ehnd_conf.ini", b"conf")
                z.writestr("translator/J2KEngine.dll", b"different-engine")
                z.writestr("translator/src.zip", b"ignore me")
            count, size = install(archive, root / "engine")
            self.assertEqual(count, 4)
            self.assertEqual(size, len(b"dummy32dependencyrulesconf"))
            self.assertEqual((root / "engine/J2KEngineH.dll").read_bytes(), b"dummy32")
            self.assertEqual((root / "engine/J2KEngine.dlx").read_bytes(), b"dependency")
            self.assertFalse((root / "engine/translator").exists())
            with self.assertRaises(FileExistsError):
                install(archive, root / "engine")

    def test_missing_dependency_rejected_without_creating_destination(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            archive = root / "input.zip"
            with ZipFile(archive, "w") as z:
                z.writestr("J2KEngine.dll", b"wrapper")
                z.writestr("Dat/RuleKJ.Dat", b"rules")
            destination = root / "engine"
            with self.assertRaisesRegex(ValueError, r"J2KEngine\.dlx"):
                install(archive, destination)
            self.assertFalse(destination.exists())

    def test_nested_engine_is_not_substituted_for_root_dependency(self):
        stream = io.BytesIO()
        with ZipFile(stream, "w") as z:
            z.writestr("J2KEngine.dll", b"wrapper")
            z.writestr("translator/J2KEngine.dll", b"different-engine")
            z.writestr("Dat/RuleKJ.Dat", b"rules")
        with ZipFile(io.BytesIO(stream.getvalue())) as z:
            with self.assertRaisesRegex(ValueError, r"J2KEngine\.dlx"):
                members_to_install(z)

    def test_missing_engine_rejected(self):
        stream = io.BytesIO()
        with ZipFile(stream, "w") as z:
            z.writestr("Dat/RuleKJ.Dat", b"rules")
        with ZipFile(io.BytesIO(stream.getvalue())) as z:
            with self.assertRaises(ValueError):
                members_to_install(z)

    def test_path_traversal_rejected(self):
        stream = io.BytesIO()
        with ZipFile(stream, "w") as z:
            z.writestr("../../J2KEngine.dll", b"unsafe")
        with ZipFile(io.BytesIO(stream.getvalue())) as z:
            with self.assertRaises(ValueError):
                members_to_install(z)


if __name__ == "__main__":
    unittest.main()
