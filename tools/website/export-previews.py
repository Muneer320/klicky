"""Build short listening examples from the bundled packs (requires FFmpeg)."""
import argparse
import array
import json
import subprocess
import sys
from pathlib import Path


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--ffmpeg", default="ffmpeg", help="FFmpeg executable")
    args = parser.parse_args()
    root = Path(__file__).resolve().parents[2]
    output = root / "assets/website/audio"
    output.mkdir(parents=True, exist_ok=True)
    rate = 44100
    for pack in sorted((root / "sounds").iterdir()):
        if not pack.is_dir():
            continue
        mapping = json.loads((pack / "config.json").read_text())["defines"]
        decoded = subprocess.check_output([
            args.ffmpeg, "-hide_banner", "-loglevel", "error", "-i",
            str(pack / "sound.ogg"), "-f", "s16le", "-ar", str(rate),
            "-ac", "1", "pipe:1",
        ])
        pcm = array.array("h", decoded)
        if sys.byteorder != "little":
            pcm.byteswap()
        timeline = [0] * int(rate * 2.4)
        for index, key in enumerate(["KeyK", "KeyL", "KeyI", "KeyC", "KeyK", "KeyY", "Space"]):
            start, duration = mapping[key]
            begin = start * rate // 1000
            samples = pcm[begin:begin + duration * rate // 1000]
            peak = max((abs(value) for value in samples), default=0)
            gain = min(6, 32767 * 0.85 / peak) if peak else 1
            offset = int((0.14 + index * 0.28) * rate)
            for frame, value in enumerate(samples):
                if offset + frame >= len(timeline):
                    break
                timeline[offset + frame] += round(value * gain * 0.8)
        rendered = array.array("h", (max(-32768, min(32767, value)) for value in timeline))
        if sys.byteorder != "little":
            rendered.byteswap()
        subprocess.run([
            args.ffmpeg, "-hide_banner", "-loglevel", "error", "-y",
            "-f", "s16le", "-ar", str(rate), "-ac", "1", "-i", "pipe:0",
            "-af", "afade=t=out:st=2.35:d=0.05", "-codec:a", "libmp3lame",
            "-b:a", "96k", "-map_metadata", "-1", str(output / f"{pack.name}.mp3"),
        ], input=rendered.tobytes(), check=True)
        print(f"Exported {pack.name}")


if __name__ == "__main__":
    main()
