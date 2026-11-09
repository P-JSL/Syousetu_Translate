import tempfile
from pathlib import Path
import unittest
from package_web_release import EXPECTED, NOTICES
from test_package_web_release import executable
from verify_web_artifact import verify


class ProductionPackageTests(unittest.TestCase):
    def test_unrequested_library_cannot_enter_the_artifact(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in EXPECTED | NOTICES:
                path = root / name
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b'fixture')
            (root / 'syousetu-gui.exe').write_bytes(executable(0x8664, 0x20B, 2))
            (root / 'syousetu-translate.exe').write_bytes(executable(0x8664, 0x20B, 3))
            verify(root)
            (root / 'syousetu_translate.dll').write_bytes(b'unused legacy library')
            with self.assertRaises(AssertionError):
                verify(root)


if __name__ == '__main__':
    unittest.main()
