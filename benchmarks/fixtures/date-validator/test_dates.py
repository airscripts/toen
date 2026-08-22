import unittest

from dates import valid_date


class DateTests(unittest.TestCase):
    def test_leap_days(self):
        self.assertTrue(valid_date("2024-02-29"))
        self.assertFalse(valid_date("2025-02-29"))

    def test_impossible_month_and_day(self):
        self.assertFalse(valid_date("2025-13-01"))
        self.assertFalse(valid_date("2025-04-31"))


if __name__ == "__main__":
    unittest.main()
