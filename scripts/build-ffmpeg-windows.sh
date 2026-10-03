#!/usr/bin/env bash
set -euo pipefail

# Native LGPL build: no GPL/nonfree/version3 code or external codec libraries.
version=8.1.3
source_sha256=7138d28c96d9d3e3af4ee3d8cad72741f8ffb40da90c1112235dea3ecd3178a3
output_dir=$(realpath -m "${1:?Usage: build-ffmpeg-windows.sh OUTPUT_DIR}")
mkdir -p "$output_dir/bin" "$output_dir/licenses" "$output_dir/source"
build_dir=$(mktemp -d)
trap 'rm -rf "$build_dir"' EXIT
archive="$output_dir/source/ffmpeg-$version.tar.xz"
curl --fail --location --retry 3 "https://ffmpeg.org/releases/ffmpeg-$version.tar.xz" --output "$archive"
printf '%s  %s\n' "$source_sha256" "$archive" | sha256sum --check --strict
tar -xf "$archive" -C "$build_dir"
cd "$build_dir/ffmpeg-$version"

configure_args=(
  --target-os=mingw32 --arch=x86_64 --enable-cross-compile
  --cross-prefix=x86_64-w64-mingw32-
  --disable-autodetect --disable-gpl --disable-nonfree --disable-version3
  --disable-shared --enable-static --disable-debug --disable-doc
  --disable-ffplay --disable-x86asm --disable-network
  --extra-ldflags=-static
)
./configure "${configure_args[@]}"
make -j"${BUILD_JOBS:-2}" ffmpeg.exe ffprobe.exe
cp ffmpeg.exe ffprobe.exe "$output_dir/bin/"
cp COPYING.* LICENSE.md "$output_dir/licenses/"
{
  printf 'FFmpeg %s\nSource: https://ffmpeg.org/releases/ffmpeg-%s.tar.xz\nSHA-256: %s\n' "$version" "$version" "$source_sha256"
  printf 'Configure:'
  printf ' %q' "${configure_args[@]}"
  printf '\n\n'
  x86_64-w64-mingw32-gcc --version
} > "$output_dir/BUILD.txt"
