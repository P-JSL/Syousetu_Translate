import struct
import unittest
from package_web_release import inspect_pe


def executable(machine, magic, subsystem):
    data = bytearray(512)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<H", data, 0x84, machine)
    struct.pack_into("<H", data, 0x94, 240 if magic == 0x20B else 224)
    struct.pack_into("<H", data, 0x98, magic)
    struct.pack_into("<H", data, 0x98 + 68, subsystem)
    return bytes(data)


class ReleaseArchitectureTests(unittest.TestCase):
    def test_x64_and_legacy_x86_executables(self):
        self.assertEqual(inspect_pe(executable(0x8664, 0x20B, 2), 2, "x64"), [])
        self.assertEqual(inspect_pe(executable(0x14C, 0x10B, 3), 3), [])

    def test_wrong_architecture_or_window_kind_is_rejected(self):
        with self.assertRaises(AssertionError):
            inspect_pe(executable(0x14C, 0x10B, 2), 2, "x64")
        with self.assertRaises(AssertionError):
            inspect_pe(executable(0x8664, 0x20B, 3), 2, "x64")
        with self.assertRaises(AssertionError):
            inspect_pe(executable(0x8664, 0x10B, 2), 2, "x64")


if __name__ == "__main__":
    unittest.main()
