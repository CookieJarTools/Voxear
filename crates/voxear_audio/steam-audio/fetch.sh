#!/bin/sh
# Fetches Steam Audio's prebuilt libraries into lib/, as Voxear loads them: the
# release named in README.md, checked against its SHA-256, Linux and Windows
# 64-bit only. They are not kept in git, being 90 MB together.
set -eu
here=$(cd "$(dirname "$0")" && pwd)
version=4.8.1
sum=4a0aa5ec1176f38f0b0993a37c2259d9e86f27e22d5e24f83ec4c3cb9a1d5449
url="https://github.com/ValveSoftware/steam-audio/releases/download/v$version/steamaudio_$version.zip"
work=$(mktemp -d)
trap 'rm -rf "$work"' EXIT
echo "Fetching Steam Audio $version"
curl -fL --proto '=https' -o "$work/steamaudio.zip" "$url"
echo "$sum  $work/steamaudio.zip" | sha256sum -c -
unzip -q "$work/steamaudio.zip" 'steamaudio/lib/linux-x64/*' 'steamaudio/lib/windows-x64/*' -d "$work"
mkdir -p "$here/lib"
rm -rf "$here/lib/linux-x64" "$here/lib/windows-x64"
cp -r "$work/steamaudio/lib/linux-x64" "$work/steamaudio/lib/windows-x64" "$here/lib/"
echo "Steam Audio $version is in $here/lib"
