import importlib.util
import tempfile
import unittest
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location("release", ROOT / "scripts/release.py")
release = importlib.util.module_from_spec(spec)
spec.loader.exec_module(release)


class PackagingTests(unittest.TestCase):
    def test_mismatched_tag_is_rejected(self):
        with self.assertRaises(ValueError):
            release.version("refs/tags/v999.0.0")
        self.assertEqual(release.version(), release.version("refs/heads/main"))

    def test_windows_variants_include_correct_tools_and_source(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            tools = root / "ffmpeg"
            for name in ("bin", "licenses", "source"):
                (tools / name).mkdir(parents=True)
            for tool in ("ffmpeg.exe", "ffprobe.exe"):
                (tools / "bin" / tool).write_bytes(b"fixture")
            (tools / "licenses/COPYING.LGPLv2.1").write_text("license")
            (tools / "source/ffmpeg-8.1.3.tar.xz").write_bytes(b"source")
            (tools / "BUILD.txt").write_text("build recipe")
            executable = root / "animeta.exe"
            executable.write_bytes(b"app")
            dist = root / "dist"
            release.windows(executable, tools, dist)
            archives = list(dist.glob("*.zip"))
            self.assertEqual(len(archives), 2)
            for archive in archives:
                bundled = "with-ffmpeg" in archive.name
                with zipfile.ZipFile(archive) as file:
                    names = file.namelist()
                    for required in ("animeta.exe", "LICENSE", "licenses/noto-sans-jp-OFL.txt"):
                        self.assertTrue(any(name.endswith(f"/{required}") for name in names))
                    for required in ("tools/ffmpeg.exe", "tools/ffprobe.exe", "ffmpeg/source/ffmpeg-8.1.3.tar.xz", "ffmpeg/build-ffmpeg-windows.sh"):
                        self.assertEqual(any(name.endswith(f"/{required}") for name in names), bundled)
            release.checksums(dist)
            self.assertEqual(len((dist / "SHA256SUMS.txt").read_text().splitlines()), 2)
            (tools / "bin/ffprobe.exe").unlink()
            with self.assertRaises(ValueError):
                release.windows(executable, tools, root / "missing-tool")

    def test_linux_requires_both_package_formats(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            with self.assertRaises(ValueError):
                release.linux(root, root / "dist")
            for folder, filename in (("deb", "app.deb"), ("appimage", "app.AppImage")):
                (root / folder).mkdir()
                (root / folder / filename).write_bytes(b"fixture")
            release.linux(root, root / "dist")
            self.assertEqual(len(list((root / "dist").iterdir())), 3)


if __name__ == "__main__":
    unittest.main()
