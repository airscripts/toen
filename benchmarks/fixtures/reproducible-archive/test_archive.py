import time
import unittest

from archive import build_archive


class ArchiveTests(unittest.TestCase):
    def test_two_runs_are_byte_identical(self):
        files = {"b.txt": b"second", "a.txt": b"first"}
        first = build_archive(files)
        time.sleep(2.1)
        second = build_archive(files)

        self.assertEqual(first, second)


if __name__ == "__main__":
    unittest.main()
