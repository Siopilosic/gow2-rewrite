"""Export the samples of an SBI_ sound-sample record (SoundServer 0x15, subtype 4) as WAV files.

  python tools/sbi_export.py extracted/pak/R_PERMA.WAD SBI_Hero analysis/audio/SBI_Hero [--rate 22050]

Layout (docs/audio.md): u16 0x15, u16 4, u32 count, then count entries of 0x1c bytes at +0x08:
{char name[24] (file name, e.g. "DIEVOC1\\0vag"), u32 offset}. Each sample is raw PS-ADPCM (Sony VAG
body without the 48-byte "VAGp" header) from its offset to the next one (or to the end).
PS-ADPCM: 16-byte frames: byte 0 = shift (low nibble) | filter (high nibble), byte 1 = flags
(bit 0 = end), 14 bytes = 28 signed 4-bit samples, low nibble first; standard filter table.
The sample rate is not stored here; 22050 Hz is the assumed default (MEDIUM).
"""
import argparse
import os
import struct
import sys
import wave

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402

FILTERS = [(0, 0), (60, 0), (115, -52), (98, -55), (122, -60)]


def decode_psadpcm(data):
    out, h1, h2 = [], 0, 0
    for i in range(0, len(data) - 15, 16):
        sf, flags = data[i], data[i + 1]
        shift, filt = sf & 0x0F, min(sf >> 4, 4)
        f0, f1 = FILTERS[filt]
        for byte in data[i + 2:i + 16]:
            for nib in (byte & 0x0F, byte >> 4):
                s = nib - 16 if nib & 8 else nib
                v = (s << 12) >> shift
                v += (h1 * f0 + h2 * f1 + 32) >> 6
                v = max(-32768, min(32767, v))
                out.append(v)
                h2, h1 = h1, v
        if flags & 1 and i > 0:
            break
    return out


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wad")
    ap.add_argument("record")
    ap.add_argument("out")
    ap.add_argument("--rate", type=int, default=22050)
    a = ap.parse_args()
    body = next(b for _o, tag, _p, n, b in wad_records(open(a.wad, "rb").read()) if n == a.record and b)
    count = struct.unpack_from("<I", body, 4)[0]
    entries = []
    for k in range(count):
        e = 8 + 0x1C * k
        name = body[e:e + 24].split(bytes(1))[0].decode("latin-1")
        entries.append((name, struct.unpack_from("<I", body, e + 24)[0]))
    os.makedirs(a.out, exist_ok=True)
    ends = [o for _n, o in entries[1:]] + [len(body)]
    for (name, off), end in zip(entries, ends):
        pcm = decode_psadpcm(body[off:end])
        with wave.open(os.path.join(a.out, name + ".wav"), "wb") as w:
            w.setnchannels(1)
            w.setsampwidth(2)
            w.setframerate(a.rate)
            w.writeframes(struct.pack(f"<{len(pcm)}h", *pcm))
        peak = max((abs(x) for x in pcm), default=0)
        print(f"{name:12s} off {off:#07x} {len(pcm):6d} samples {len(pcm) / a.rate:5.2f}s peak {peak}")


if __name__ == "__main__":
    main()
