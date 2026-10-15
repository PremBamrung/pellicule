#!/bin/sh
# Regenerates the tiny image fixtures used by the derivative tests
# (src/derive.rs). Needs the `vips` CLI and python3. The outputs are
# committed; rerun only to change them.
set -eu

out="$(cd "$(dirname "$0")/.." && pwd)/tests/fixtures"
tmp="$(mktemp -d)"
trap 'rm -rf "$tmp"' EXIT
mkdir -p "$out"

solid() { # name width height r g b
    vips black "$tmp/b.v" "$2" "$3" --bands 3
    vips linear "$tmp/b.v" "$tmp/$1.v" "1 1 1" "$4 $5 $6" --uchar
}

# Display-P3 encoded red with the profile embedded. Larger than `m` so the
# smaller variants are real downscales.
solid red 1200 800 220 30 30
vips icc_transform "$tmp/red.v" "$tmp/p3.v" p3 --input-profile srgb
vips copy "$tmp/p3.v" "$out/p3.jpg[Q=95]"

# Stored 64×32, red left / blue right, tagged EXIF orientation 6 (rotate
# 90° clockwise): displayed 32×64, red on top.
solid l 32 32 220 30 30
solid r 32 32 30 30 220
vips join "$tmp/l.v" "$tmp/r.v" "$tmp/lr.v" horizontal
vips copy "$tmp/lr.v" "$tmp/rotated.jpg[Q=95,keep=none]"
python3 - "$tmp/rotated.jpg" "$out/rotated.jpg" <<'EOF'
import struct, sys
data = open(sys.argv[1], "rb").read()
assert data[:2] == b"\xff\xd8"
# Big-endian TIFF, one IFD entry: Orientation (0x0112), SHORT, 1, value 6.
tiff = b"MM\x00\x2a" + struct.pack(">I", 8) + struct.pack(">H", 1)
tiff += struct.pack(">HHIHH", 0x0112, 3, 1, 6, 0) + struct.pack(">I", 0)
app1 = b"Exif\x00\x00" + tiff
segment = b"\xff\xe1" + struct.pack(">H", len(app1) + 2) + app1
open(sys.argv[2], "wb").write(data[:2] + segment + data[2:])
EOF

# Grayscale with alpha, no profile.
vips black "$tmp/g.v" 48 24 --bands 2
vips linear "$tmp/g.v" "$tmp/ga.v" "1 1" "128 200" --uchar
vips copy "$tmp/ga.v" "$out/gray-alpha.png" --interpretation b-w

# Smaller than every variant: must not be upscaled.
solid tiny 40 30 90 140 60
vips copy "$tmp/tiny.v" "$out/tiny.jpg[Q=90,keep=none]"

ls -l "$out"
