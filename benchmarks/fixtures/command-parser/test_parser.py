import unittest

from parser import parse


class ParserTests(unittest.TestCase):
    def test_unknown_argument_preserves_active_mode(self):
        self.assertEqual(parse("$toen boh", "arranda"), ("usage", "arranda"))

    def test_known_mode_still_activates(self):
        self.assertEqual(parse("$toen ammodino", "spento"), ("activated", "ammodino"))


if __name__ == "__main__":
    unittest.main()
