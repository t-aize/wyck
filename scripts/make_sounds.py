#!/usr/bin/env python3
"""Writes the alert sounds of the app to crates/wyck/assets/sounds.

The sounds are synthesized here (sine partials with an envelope), so they belong to this project
and carry no third party license. Run it again to rebuild them; the output is deterministic.

    python3 scripts/make_sounds.py
"""

import math
import struct
import wave
from pathlib import Path

RATE = 44_100
OUT = Path(__file__).resolve().parent.parent / "crates" / "wyck" / "assets" / "sounds"


def tone(freq, length, partials=((1.0, 1.0),), attack=0.004, decay=6.0, start=0.0, total=None):
    """One decaying note: a list of (frequency ratio, level) partials, a soft attack, an
    exponential decay. `start` delays it inside a buffer of `total` seconds."""
    total = total if total is not None else start + length
    samples = [0.0] * int(RATE * total)
    first = int(RATE * start)
    count = int(RATE * length)
    norm = sum(level for _, level in partials)
    for i in range(count):
        t = i / RATE
        env = min(1.0, t / attack) * math.exp(-decay * t)
        fade = min(1.0, (count - i) / (RATE * 0.01))
        value = sum(level * math.sin(2 * math.pi * freq * ratio * t) for ratio, level in partials)
        if first + i < len(samples):
            samples[first + i] += value / norm * env * fade
    return samples


def mix(*parts):
    length = max(len(p) for p in parts)
    out = [0.0] * length
    for part in parts:
        for i, v in enumerate(part):
            out[i] += v
    return out


def write(name, samples, gain=0.6):
    peak = max(abs(s) for s in samples) or 1.0
    scale = gain / peak
    OUT.mkdir(parents=True, exist_ok=True)
    with wave.open(str(OUT / f"{name}.wav"), "wb") as f:
        f.setnchannels(1)
        f.setsampwidth(2)
        f.setframerate(RATE)
        f.writeframes(b"".join(struct.pack("<h", int(max(-1.0, min(1.0, s * scale)) * 32767)) for s in samples))


BELL = ((1.0, 1.0), (2.0, 0.35), (3.01, 0.15))
GLASS = ((1.0, 1.0), (2.76, 0.4), (5.4, 0.18))

# A short high ping.
write("ping", tone(1568.0, 0.45, BELL, decay=9.0))
# Two rising notes.
write("chime", mix(tone(784.0, 0.5, BELL, decay=7.0, total=0.75),
                   tone(1174.7, 0.5, BELL, decay=7.0, start=0.16, total=0.75)))
# A soft low pulse.
write("pulse", tone(392.0, 0.5, ((1.0, 1.0), (2.0, 0.2)), attack=0.02, decay=8.0))
# A round drop that falls.
drop = []
for i in range(int(RATE * 0.4)):
    t = i / RATE
    # The frequency is 300 + 1200 * exp(-5t) Hz: its integral is the phase.
    phase = 2 * math.pi * (1200.0 / 5.0 * (1 - math.exp(-5.0 * t)) + 300.0 * t)
    drop.append(math.sin(phase) * min(1.0, t / 0.004) * math.exp(-7.0 * t))
write("drop", drop)
# Three quick notes, for what should not be missed.
write("alert", mix(tone(880.0, 0.22, GLASS, decay=10.0, total=0.7),
                   tone(880.0, 0.22, GLASS, decay=10.0, start=0.2, total=0.7),
                   tone(1318.5, 0.4, GLASS, decay=8.0, start=0.4, total=0.8)))
# A glass tap.
write("glass", tone(2093.0, 0.35, GLASS, attack=0.002, decay=12.0))
print(f"wrote {len(list(OUT.glob('*.wav')))} sounds to {OUT}")
