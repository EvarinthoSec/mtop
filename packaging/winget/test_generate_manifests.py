from __future__ import annotations

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("generate_manifests.py")
class GenerateManifestsTests(unittest.TestCase):
    def test_generates_three_consistent_portable_manifests(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)
            subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--version",
                    "0.1.0-alpha.1",
                    "--installer-url",
                    "https://github.com/EvarinthoSec/mtop/releases/download/v0.1.0-alpha.1/mtop-windows-x86_64-0.1.0-alpha.1.zip",
                    "--installer-sha256",
                    "A" * 64,
                    "--output",
                    str(output),
                ],
                check=True,
            )
            manifests = output / "e" / "EvarinthoSec" / "mtop" / "0.1.0-alpha.1"
            self.assertEqual(len(list(manifests.glob("*.yaml"))), 3)
            installer = (manifests / "EvarinthoSec.mtop.installer.yaml").read_text()
            self.assertIn("InstallerType: zip", installer)
            self.assertIn("NestedInstallerType: portable", installer)
            self.assertIn("InstallerSha256: " + "a" * 64, installer)
            self.assertIn("PortableCommandAlias: mtop", installer)
            self.assertIn("ManifestVersion: 1.12.0", installer)

    def test_rejects_non_release_installer_urls(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = subprocess.run(
                [
                    sys.executable,
                    str(SCRIPT),
                    "--version",
                    "0.1.0",
                    "--installer-url",
                    "https://example.com/mtop.zip",
                    "--installer-sha256",
                    "a" * 64,
                    "--output",
                    temporary,
                ],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("installer URL must be", result.stderr)


if __name__ == "__main__":
    unittest.main()
