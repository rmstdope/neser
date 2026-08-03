"""Visualize raw 8-bit audio samples or PCM WAV files.

This script reads a raw audio dump where each sample is a single unsigned
8-bit byte (0-255) or a PCM WAV file (8/16/24/32-bit) and plots it as a
waveform. By default, it looks for samples.raw in the current working
directory and plots up to 20,000 samples starting at index 0. You can change
the plotted window, normalize the values to [-1, 1], or point to a different
file using the CLI arguments below.

CLI parameters (with defaults):
    - path: samples.raw
    - --max-samples: 20000
    - --start: 0
    - --normalize: False
"""

from __future__ import annotations

import argparse
import wave
from pathlib import Path

import matplotlib.pyplot as plt


def _clamp(value: float, min_value: float, max_value: float) -> float:
    return max(min_value, min(value, max_value))


def _float_to_u8(sample: float) -> int:
    """Convert a normalized sample in [-1, 1] to unsigned 8-bit [0, 255]."""
    return round((_clamp(sample, -1.0, 1.0) + 1.0) * 127.5)


def _read_wav_samples(path: Path) -> list[int]:
    """Load PCM WAV samples and return unsigned 8-bit values.

    Averages channels to mono and converts to 0-255.
    """
    with wave.open(str(path), "rb") as wav:
        channels = wav.getnchannels()
        sample_width = wav.getsampwidth()
        frame_count = wav.getnframes()
        frames = wav.readframes(frame_count)

    if channels < 1:
        raise SystemExit("WAV file has no channels")
    if sample_width not in {1, 2, 3, 4}:
        raise SystemExit(f"Unsupported WAV sample width: {sample_width}")

    bytes_per_frame = sample_width * channels
    if len(frames) % bytes_per_frame != 0:
        raise SystemExit("WAV data size does not align with frame size")

    max_int = float(2 ** (sample_width * 8 - 1))
    samples: list[int] = []
    for frame_start in range(0, len(frames), bytes_per_frame):
        frame = frames[frame_start : frame_start + bytes_per_frame]
        accum = 0.0
        for channel_index in range(channels):
            start = channel_index * sample_width
            chunk = frame[start : start + sample_width]
            if sample_width == 1:
                value = (chunk[0] - 128) / 128.0
            else:
                if sample_width == 3:
                    raw = int.from_bytes(chunk, byteorder="little", signed=False)
                    if raw & 0x800000:
                        raw -= 1 << 24
                    value = raw / max_int
                else:
                    raw = int.from_bytes(chunk, byteorder="little", signed=True)
                    value = raw / max_int
            accum += value
        mono = accum / channels
        samples.append(_float_to_u8(mono))

    return samples


def load_samples(path: Path) -> list[int]:
    """Load raw 8-bit samples or PCM WAV samples from a file.

    Parameters:
        path: Path to the sample file.
    """
    if path.suffix.lower() == ".wav":
        return _read_wav_samples(path)
    data = path.read_bytes()
    return list(data)


def normalize_unsigned_8bit(samples: list[int]) -> list[float]:
    """Normalize unsigned 8-bit samples to a float range of [-1, 1].

    Parameters:
        samples: List of 0-255 sample values.
    """
    if not samples:
        return []
    return [((value - 128) / 128.0) for value in samples]


def parse_args() -> argparse.Namespace:
    """Parse CLI arguments for the waveform plot.

    Parameters (defaults):
        path: samples.raw
        --max-samples: 20000
        --start: 0
        --normalize: False
    """
    parser = argparse.ArgumentParser(description="Plot raw 8-bit audio samples from a .raw or .wav file.")
    parser.add_argument(
        "path",
        nargs="?",
        default="samples.raw",
        help="Path to raw or wav sample file (default: samples.raw)",
    )
    parser.add_argument(
        "--max-samples",
        type=int,
        default=20_000,
        help="Maximum number of samples to plot (default: 20000)",
    )
    parser.add_argument(
        "--start",
        type=int,
        default=0,
        help="Start index for plotting (default: 0)",
    )
    parser.add_argument(
        "--normalize",
        action="store_true",
        help="Normalize unsigned 8-bit samples to [-1, 1]",
    )
    return parser.parse_args()


def main() -> None:
    """Load samples, slice a plotting window, and render the waveform."""
    args = parse_args()
    path = Path(args.path)
    if not path.exists():
        raise SystemExit(f"File not found: {path}")

    samples = load_samples(path)
    if args.start < 0 or args.start >= len(samples):
        raise SystemExit("Start index out of range")

    end = min(len(samples), args.start + args.max_samples)
    window = samples[args.start : end]

    if args.normalize:
        values = normalize_unsigned_8bit(window)
        ylabel = "Amplitude (normalized)"
    else:
        values = window
        ylabel = "Amplitude (0-255)"

    x_values = list(range(args.start, args.start + len(values)))

    plt.figure(figsize=(12, 4))
    plt.plot(x_values, values, linewidth=1.0)
    plt.title(f"Audio samples: {path.name}")
    plt.xlabel("Sample index")
    plt.ylabel(ylabel)
    plt.grid(True, alpha=0.3)
    plt.tight_layout()
    plt.show()


if __name__ == "__main__":
    main()
