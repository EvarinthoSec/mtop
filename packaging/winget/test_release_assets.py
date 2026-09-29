from __future__ import annotations

import subprocess
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).parents[1] / "normalize-release-assets.sh"


class NormalizeReleaseAssetsTests(unittest.TestCase):
    def test_normalizes_all_platform_assets_without_renaming_package_metadata(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            dist = Path(temporary)
            inputs = {
                "mtop_0.1.0.alpha.1_amd64.deb",
                "mtop-0.1.0.alpha.1-1.x86_64.rpm",
                "mtop-0.1.0_alpha.1-1-x86_64.pkg.tar.zst",
                "mtop-debug-0.1.0_alpha.1-1-x86_64.pkg.tar.zst",
                "mtop-0.1.0_alpha1-r0.apk",
                "mtop-doc-0.1.0_alpha1-r0.apk",
                "-6abbc312.rsa.pub",
                "mtop-macos-arm64-0.1.0-alpha.1.tar.gz",
                "mtop-macos-x86_64-0.1.0-alpha.1.tar.gz",
                "mtop-windows-x86_64-0.1.0-alpha.1.zip",
            }
            for filename in inputs:
                (dist / filename).write_bytes(b"package fixture")

            subprocess.run(["bash", str(SCRIPT), "0.1.0-alpha.1", str(dist)], check=True)

            expected = {
                "mtop-linux-amd64-0.1.0-alpha.1.deb",
                "mtop-linux-x86_64-0.1.0-alpha.1.rpm",
                "mtop-linux-x86_64-0.1.0-alpha.1.pkg.tar.zst",
                "mtop-linux-x86_64-0.1.0-alpha.1-debug.pkg.tar.zst",
                "mtop-linux-x86_64-0.1.0-alpha.1.apk",
                "mtop-linux-x86_64-0.1.0-alpha.1-docs.apk",
                "mtop-alpine-signing-key-0.1.0-alpha.1.rsa.pub",
                "mtop-macos-arm64-0.1.0-alpha.1.tar.gz",
                "mtop-macos-x86_64-0.1.0-alpha.1.tar.gz",
                "mtop-windows-x86_64-0.1.0-alpha.1.zip",
            }
            self.assertEqual({path.name for path in dist.iterdir()}, expected)
            for path in dist.iterdir():
                self.assertEqual(path.read_bytes(), b"package fixture")

    def test_rejects_unexpected_package_set(self) -> None:
        with tempfile.TemporaryDirectory() as temporary:
            result = subprocess.run(
                ["bash", str(SCRIPT), "0.1.0", temporary],
                capture_output=True,
                text=True,
            )
            self.assertNotEqual(result.returncode, 0)
            self.assertIn("unexpected release input counts", result.stderr)


if __name__ == "__main__":
    unittest.main()
