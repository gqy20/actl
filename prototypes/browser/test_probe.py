import json
import os
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

from probe import run


class RunnerTests(unittest.TestCase):
    def test_stop_precedes_dependency_and_process_start(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            stop = root / "actl" / "stop-requested"
            stop.parent.mkdir()
            stop.write_text("preserve")
            output = root / "report"
            with patch.dict(os.environ, {"LOCALAPPDATA": str(root)}), patch("probe.subprocess.Popen") as launch:
                report = run("missing-browser.exe", output)
            launch.assert_not_called()
            self.assertEqual(report["error"]["code"], "ABORTED")
            self.assertEqual(report["cleanup"], "not_started")
            self.assertFalse((output / "profile").exists())
            self.assertEqual(stop.read_text(), "preserve")
            self.assertEqual(json.loads((output / "report.json").read_text()), report)
