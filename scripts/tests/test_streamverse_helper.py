from __future__ import annotations

import sys
import unittest
from pathlib import Path

SCRIPTS_DIR = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(SCRIPTS_DIR))

import streamverse_helper


class HelperCommandTests(unittest.TestCase):
    def test_health_command(self) -> None:
        self.assertEqual(streamverse_helper.main(["health"]), 0)

    def test_unknown_command_is_rejected(self) -> None:
        self.assertEqual(streamverse_helper.main(["unknown"]), 2)


if __name__ == "__main__":
    unittest.main()
