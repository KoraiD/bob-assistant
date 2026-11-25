#!/usr/bin/env bash
# Records the README GIFs (see scripts/record-gifs.swift) and converts them with ffmpeg into assets/.
# Run it with BOB running, a Focus mode on, and nothing private near the top-left of the screen.
set -euo pipefail
cd "$(dirname "$0")/.."
command -v ffmpeg >/dev/null || { echo "ffmpeg is needed: brew install ffmpeg"; exit 1; }
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
swiftc -O scripts/record-gifs.swift -o "$tmp/record-gifs"
"$tmp/record-gifs" "$tmp"
for name in ghost-text selection-actions; do
  ffmpeg -loglevel error -y -i "$tmp/$name.mov" \
    -vf "fps=12,scale=720:-1:flags=lanczos,split[a][b];[a]palettegen=max_colors=96:stats_mode=diff[p];[b][p]paletteuse=dither=bayer:bayer_scale=4" \
    "assets/$name.gif"
  echo "assets/$name.gif: $(du -h "assets/$name.gif" | cut -f1)"
done
