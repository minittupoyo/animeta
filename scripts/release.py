"""Version validation and release packaging; Python standard library only."""

import argparse
import hashlib
import json
import re
import shutil
import tarfile
import tempfile
import tomllib
import zipfile
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]


def version(ref=None):
    versions = [
        json.loads((ROOT / "package.json").read_text())["version"],
        json.loads((ROOT / "src-tauri/tauri.conf.json").read_text())["version"],
        tomllib.loads((ROOT / "src-tauri/Cargo.toml").read_text())["package"]["version"],
    ]
    if len(set(versions)) != 1 or not re.fullmatch(
        r"\d+\.\d+\.\d+(?:-[0-9A-Za-z]+(?:[.-][0-9A-Za-z]+)*)?", versions[0]
    ):
        raise ValueError("package.json / tauri.conf.json / Cargo.toml versions must match")
    if ref and ref.startswith("refs/tags/") and ref != f"refs/tags/v{versions[0]}":
        raise ValueError(f"Tag must match app version: v{versions[0]}")
    return versions[0]


def copy_notices(destination):
    for name in ("LICENSE", "THIRD_PARTY_NOTICES.md"):
        shutil.copy2(ROOT / name, destination / name)
    shutil.copytree(ROOT / "licenses", destination / "licenses")


def archive_zip(folder, output):
    with zipfile.ZipFile(output, "w", zipfile.ZIP_DEFLATED) as archive:
        for file in sorted(folder.rglob("*")):
            if file.is_file():
                archive.write(file, file.relative_to(folder.parent))


def windows(executable, ffmpeg_dir, dist):
    app_version = version()
    dist.mkdir(parents=True, exist_ok=True)
    with tempfile.TemporaryDirectory() as temporary:
        base = Path(temporary) / f"animeta-{app_version}-windows-x64"
        base.mkdir()
        shutil.copy2(executable, base / "animeta.exe")
        copy_notices(base)
        (base / "README.txt").write_text(
            "Animeta Windows x64\n\n"
            "ZIPをすべて展開してanimeta.exeを起動してください。Microsoft Edge WebView2 Runtimeが必要です。\n"
            "この版はFFmpeg/ffprobe非同梱です。別途インストールし、PATHまたはアプリの設定で指定してください。\n",
            encoding="utf-8",
        )
        archive_zip(base, dist / f"{base.name}.zip")
        bundled = base.with_name(f"{base.name}-with-ffmpeg")
        base.rename(bundled)
        shutil.copytree(ffmpeg_dir / "bin", bundled / "tools")
        for tool in ("ffmpeg.exe", "ffprobe.exe"):
            if not (bundled / "tools" / tool).is_file():
                raise ValueError(f"Bundled tool missing: {tool}")
        notices = bundled / "ffmpeg"
        shutil.copytree(ffmpeg_dir / "licenses", notices / "licenses")
        shutil.copytree(ffmpeg_dir / "source", notices / "source")
        shutil.copy2(ffmpeg_dir / "BUILD.txt", notices / "BUILD.txt")
        shutil.copy2(ROOT / "scripts/build-ffmpeg-windows.sh", notices / "build-ffmpeg-windows.sh")
        (bundled / "README.txt").write_text(
            "Animeta Windows x64 — FFmpeg/ffprobe同梱版\n\n"
            "ZIPをすべて展開してanimeta.exeを起動してください。Microsoft Edge WebView2 Runtimeが必要です。\n"
            "既定の設定ではtoolsフォルダのFFmpeg/ffprobeを使用します。設定済みのカスタムパスは優先されます。\n"
            "FFmpegは独立したLGPL 2.1以降の実行ファイルです。ソース、ライセンス、ビルド手順はffmpegフォルダにあります。\n",
            encoding="utf-8",
        )
        with (bundled / "THIRD_PARTY_NOTICES.md").open("a", encoding="utf-8") as file:
            file.write("\nこの配布にはFFmpeg/ffprobeを同梱しています。ライセンス・ソース・ビルド手順は `ffmpeg/` を参照してください。\n")
        archive_zip(bundled, dist / f"{bundled.name}.zip")


def linux(bundle_dir, dist):
    app_version = version()
    dist.mkdir(parents=True, exist_ok=True)
    for pattern, name in [
        ("deb/*.deb", f"animeta-{app_version}-linux-amd64.deb"),
        ("appimage/*.AppImage", f"animeta-{app_version}-linux-x86_64.AppImage"),
    ]:
        matches = list(bundle_dir.glob(pattern))
        if len(matches) != 1:
            raise ValueError(f"Expected exactly one {pattern}: {matches}")
        shutil.copy2(matches[0], dist / name)
    with tarfile.open(dist / f"animeta-{app_version}-notices.tar.gz", "w:gz") as archive:
        for name in ("LICENSE", "THIRD_PARTY_NOTICES.md", "licenses"):
            archive.add(ROOT / name, arcname=name)


def checksums(dist):
    files = sorted(path for path in dist.iterdir() if path.is_file() and path.name != "SHA256SUMS.txt")
    if not files:
        raise ValueError("No release assets found")
    rows = []
    for path in files:
        with path.open("rb") as file:
            rows.append(f"{hashlib.file_digest(file, 'sha256').hexdigest()}  {path.name}\n")
    (dist / "SHA256SUMS.txt").write_text("".join(rows))


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest="command", required=True)
    validate = commands.add_parser("version")
    validate.add_argument("--ref")
    win = commands.add_parser("windows")
    win.add_argument("--exe", type=Path, required=True)
    win.add_argument("--ffmpeg-dir", type=Path, required=True)
    win.add_argument("--dist", type=Path, required=True)
    lin = commands.add_parser("linux")
    lin.add_argument("--bundle-dir", type=Path, required=True)
    lin.add_argument("--dist", type=Path, required=True)
    sums = commands.add_parser("checksums")
    sums.add_argument("--dist", type=Path, required=True)
    args = parser.parse_args()
    if args.command == "version":
        print(f"version={version(args.ref)}")
    elif args.command == "windows":
        windows(args.exe, args.ffmpeg_dir, args.dist)
    elif args.command == "linux":
        linux(args.bundle_dir, args.dist)
    else:
        checksums(args.dist)
