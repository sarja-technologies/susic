#!/usr/bin/env bash
# Generates clearly synthetic test audio (sine tones) with tags, cover art and
# an LRC sidecar, for manual and automated testing. Requires ffmpeg.
set -euo pipefail
OUT="${1:-./fixtures}"
mkdir -p "$OUT/Synth Artist/Test Album One" "$OUT/Tone Duo/Second Album"
gen() { # file freq secs title artist album track [cover]
  local f="$1" freq="$2" secs="$3" title="$4" artist="$5" album="$6" track="$7"
  local args=(-y -loglevel error -f lavfi -i "sine=frequency=$freq:duration=$secs")
  ffmpeg "${args[@]}" -metadata title="$title" -metadata artist="$artist" -metadata album="$album" \
    -metadata track="$track" -metadata date=2024 -metadata album_artist="$artist" "$f"
}
cover() { ffmpeg -y -loglevel error -f lavfi -i "color=c=$2:s=600x600:d=1" -frames:v 1 "$1"; }
cover "$OUT/c1.jpg" 0x7c3aed; cover "$OUT/c2.jpg" 0x2563eb
A="$OUT/Synth Artist/Test Album One"; B="$OUT/Tone Duo/Second Album"
ffmpeg -y -loglevel error -f lavfi -i "sine=frequency=261:duration=30" -i "$OUT/c1.jpg" -map 0 -map 1 -c:a libmp3lame -id3v2_version 3 \
  -metadata title="Tone C4" -metadata artist="Synth Artist" -metadata album="Test Album One" -metadata track=1 -metadata date=2024 \
  -metadata:s:v title="Album cover" -disposition:v attached_pic "$A/01 Tone C4.mp3"
gen "$A/02 Tone E4.flac" 329 25 "Tone E4" "Synth Artist" "Test Album One" 2
gen "$A/03 Tone G4.wav" 392 20 "Tone G4" "Synth Artist" "Test Album One" 3
gen "$B/01 Pulse A4.ogg" 440 28 "Pulse A4" "Tone Duo" "Second Album" 1
gen "$B/02 Pulse B4.m4a" 494 22 "Pulse B4" "Tone Duo" "Second Album" 2
gen "$B/03 Pulse D5.opus" 587 18 "Pulse D5" "Tone Duo" "Second Album" 3
cat > "$A/01 Tone C4.lrc" <<'LRC'
[ti:Tone C4]
[00:00.50]A steady synthetic tone
[00:05.00]Holds the middle C
[00:10.00]No melody, just a test
[00:15.00]Of synchronised lines
[00:20.00]Scrolling smoothly by
LRC
rm -f "$OUT"/c1.jpg "$OUT"/c2.jpg
echo "Fixtures written to $OUT"
