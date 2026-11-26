#!/bin/sh
# Records a draft with words and a pasted image in one tab, a switch to the next tab at 2.2 s
# and back at 3.4 s on the fixture, at 120 fps, plus a quarter-speed cut for review.
set -eu
OUT=${1:-captures/draft-attachments}
mkdir -p "$OUT"
python3 - "$OUT/picture.png" <<'EOF'
import struct, sys, zlib
w, h = 320, 200
rows = b"".join(b"\0" + b"".join(bytes((40 + x // 3, 90 + y // 3, 200)) for x in range(w)) for y in range(h))
chunk = lambda t, d: struct.pack(">I", len(d)) + t + d + struct.pack(">I", zlib.crc32(t + d))
png = b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", w, h, 8, 2, 0, 0, 0)) + chunk(b"IDAT", zlib.compress(rows)) + chunk(b"IEND", b"")
open(sys.argv[1], "wb").write(png)
EOF
cat > "$OUT/script.json" <<EOF
{ "events": [
  { "t": 0.3, "type": "tap", "key": "prompt" },
  { "t": 0.5, "type": "type", "text": "Draft in this tab", "every": 0.03 },
  { "t": 1.3, "paste": { "png": "$OUT/picture.png" } },
  { "t": 2.2, "type": "tap", "x": 370, "y": 20 },
  { "t": 3.4, "type": "tap", "x": 230, "y": 20 },
  { "t": 3.6, "type": "move", "x": 700, "y": 300 }
] }
EOF
cargo build -p brocode-desktop
TARGET=$(cargo metadata --format-version 1 --no-deps | python3 -c 'import json,sys; print(json.load(sys.stdin)["target_directory"])')
"$TARGET/debug/brocode-desktop" --fixture apps/desktop/assets/fixture.json \
  --scene session --script "$OUT/script.json" --pointer \
  --size 1280x820 --scale 2 --theme dark --frames 540 --fps 120 \
  --video "$OUT/draft-attachments.mp4"
ffmpeg -y -loglevel error -i "$OUT/draft-attachments.mp4" -vf "setpts=4*PTS" -r 30 "$OUT/draft-attachments-4x.mp4"
