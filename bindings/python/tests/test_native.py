import unittest

from wavecode_native import WaveCode, WaveCodeError


class NativeRoundTripTests(unittest.TestCase):
    @classmethod
    def setUpClass(cls):
        cls.api = WaveCode()

    def test_url_symbols_png_and_scan(self):
        url = "https://music.youtube.com/watch?v=p-LBiCtzXu0&t=59"
        code = self.api.encode(url)
        self.assertEqual(len(code), 34)
        self.assertEqual(self.api.format_version, 1)
        self.assertEqual(self.api.decode(code), url)
        png = self.api.render_png(code, 900, 240)
        self.assertTrue(png.startswith(b"\x89PNG\r\n\x1a\n"))
        self.assertEqual(self.api.scan(png), url)
        found = self.api.scan_all(png)
        self.assertEqual(found[0]["url"], url)
        self.assertEqual(found[0]["orientation"], "upright")

    def test_explicit_timestamp_and_svg(self):
        code = self.api.encode("https://music.youtube.com/watch?v=p-LBiCtzXu0&t=16", moment=59)
        self.assertTrue(self.api.decode(code).endswith("t=59"))
        self.assertIn('<svg', self.api.render_svg(code))

    def test_invalid_arguments(self):
        with self.assertRaises(ValueError):
            self.api.encode("https://example.com", moment=70000)
        with self.assertRaises(WaveCodeError):
            self.api.encode("https://example.com")
        with self.assertRaises(WaveCodeError):
            self.api.scan(b"not an image")


if __name__ == "__main__":
    unittest.main()
