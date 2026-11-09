import tempfile
from pathlib import Path
import unittest
from validate_kakuyomu_bilingual import validate


class BilingualValidationTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.output = self.root / "novel.txt"
        for n in (1, 2):
            (self.root / f"{n}.html").write_text(f"<h1 class='widget-episodeTitle'>章{n}</h1><div class='widget-episodeBody'><p>本文{n}</p><p>漢<ruby>字<rt>じ</rt></ruby></p></div>", encoding="utf-8")
        self.complete = "header\n\n[1/2] 제목1 / 章1\n\n本文1\n본문1\n漢字\n한자\n\n\n[2/2] 제목2 / 章2\n\n本文2\n본문2\n漢字\n한자\n"
        self.output.write_text(self.complete, encoding="utf-8")

    def tearDown(self):
        self.temp.cleanup()

    def test_complete_ordered_bilingual_output(self):
        result = validate(self.root, self.output, 2)
        self.assertTrue(result["complete"])
        self.assertEqual(result["source_lines"], 4)

    def test_missing_or_reordered_chapters_are_rejected(self):
        for body in [self.complete.replace("[2/2]", "[1/2]"), self.complete.replace("[1/2]", "[3/2]")]:
            self.output.write_text(body, encoding="utf-8")
            with self.assertRaises(AssertionError):
                validate(self.root, self.output, 2)

    def test_windows_crlf_is_valid_and_preserves_the_actual_file_hash(self):
        import hashlib
        data = self.complete.replace("\n", "\r\n").encode("utf-8")
        self.output.write_bytes(data)
        result = validate(self.root, self.output, 2)
        self.assertTrue(result["complete"])
        self.assertEqual(result["sha256"], hashlib.sha256(data).hexdigest())

    def test_modified_source_and_missing_translation_are_rejected(self):
        for old, new in [("本文2\n본문2", "本文CHANGED\n본문2"), ("本文1\n본문1", "本文1\n"), ("漢字\n한자", "")]:
            self.output.write_text(self.complete.replace(old, new), encoding="utf-8")
            with self.assertRaises(AssertionError):
                validate(self.root, self.output, 2)


if __name__ == "__main__":
    unittest.main()
