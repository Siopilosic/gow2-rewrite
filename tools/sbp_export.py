"""Decode an SBP_ sound-bank record (SoundServer 0x15, subtype 0) and export its sounds by SND_ name.

  python tools/sbp_export.py extracted/pak/R_PERMA.WAD SBP_general analysis/audio/SBP_general_named

Layout (docs/audio.md §4):
  record: u16 0x15, u16 0, u32 count, count x {char name[24], u32 sound index};
          preamble {3, 2, 0x18, header size, sample offset, sample size}; the 989snd "SBlk" bank
          starts at preamble + 0x18, its sample area at preamble + sample offset.
  SBlk v3 header: +0x00 'SBlk', +0x04 version 3, +0x0c name, +0x16 u16 sound count,
          +0x18 u16 grain count, +0x1c sound table offset, +0x20 grain table offset,
          +0x24 (unknown, 0x5040), +0x28/+0x2c sample size, +0x34 grain data offset (all relative to the bank).
  sound (12 bytes): s8 vol, s8 vol group, s16 pan, s8 grain count, s8 instance limit, u16 flags,
          u32 first grain (byte offset into the grain table).
  grain (8 bytes): u32 (type << 24 | 24-bit argument), s32 delay. Type 1 (tone): argument = offset of
          a 24-byte tone in the grain data. Other types are control opcodes (GRAIN_TYPES).
  tone (24 bytes): s8 priority, s8 vol, s8 center note, s8 center fine, s16 pan, s8 map low/high,
          s8 pitch bend low/high, u16 ADSR1, u16 ADSR2, u16 flags, u32 sample offset, u32 sample size.

Playback rate (MEDIUM): a sound plays at note 60; a negative center note marks a 44.1 kHz reference,
so rate = 44100 * 2 ** ((60 - |center note| - center fine / 128) / 12) (48000 for a positive note).
Writes one WAV per distinct tone of each sound (<SND name>[_<n>].wav) and sounds.tsv with the grain
programs.
"""
import argparse
import os
import struct
import sys
import wave

sys.path.insert(0, os.path.dirname(__file__))
from dcparse import wad_records  # noqa: E402
from sbi_export import decode_psadpcm  # noqa: E402

# grain opcodes as in the 989snd library (names from its later reimplementations; numbering checked
# against this bank's programs: docs/audio.md §4.2)
GRAIN_TYPES = {
    0: "null", 1: "tone", 2: "xref_id", 3: "xref_num", 4: "lfo", 5: "start_child", 6: "stop_child",
    7: "plugin_msg", 8: "branch", 20: "control_null", 21: "loop_start", 22: "loop_end",
    23: "loop_continue", 24: "stop", 25: "rand_play", 26: "rand_delay", 27: "rand_pb", 28: "pb",
    29: "add_pb", 30: "set_reg", 31: "set_reg_rand", 32: "inc_reg", 33: "dec_reg", 34: "test_reg",
    35: "marker", 36: "goto_marker", 37: "goto_rand_marker", 38: "wait_voices", 39: "play_cycle",
    40: "add_reg", 41: "key_off", 42: "kill_voices", 43: "on_stop_marker", 44: "copy_reg",
}


def tone_rate(note, fine):
    base = 44100 if note < 0 else 48000
    return base * 2 ** ((60 - abs(note) - fine / 128) / 12)


def parse(body):
    count = struct.unpack_from("<I", body, 4)[0]
    names = {}
    for k in range(count):
        e = 8 + 0x1C * k
        names[struct.unpack_from("<I", body, e + 24)[0]] = body[e:e + 24].split(bytes(1))[0].decode("latin-1")
    pre = 8 + 0x1C * count
    _v, _n, _h, _hsize, smp_off, smp_size = struct.unpack_from("<6I", body, pre)
    bank = pre + 0x18
    assert body[bank:bank + 4] == b"SBlk", body[bank:bank + 4]
    nsnd, ngrain = struct.unpack_from("<HH", body, bank + 0x16)
    snd_off, grain_off = struct.unpack_from("<II", body, bank + 0x1C)
    data_off = struct.unpack_from("<I", body, bank + 0x34)[0]
    samples = body[pre + smp_off:pre + smp_off + smp_size]
    sounds = []
    for s in range(nsnd):
        vol, vgrp, pan, ng, inst, flags, first = struct.unpack_from("<bbhbbHI", body, bank + snd_off + 12 * s)
        grains = []
        for j in range(ng):
            w, delay = struct.unpack_from("<Ii", body, bank + grain_off + first + 8 * j)
            typ, arg = w >> 24, w & 0xFFFFFF
            tone = None
            if typ == 1:
                f = struct.unpack_from("<bbbbhbbbbHHHII", body, bank + data_off + arg)
                tone = dict(prio=f[0], vol=f[1], note=f[2], fine=f[3], pan=f[4], adsr1=f[9], adsr2=f[10],
                            flags=f[11], offset=f[12], size=f[13])
            elif typ == 8:  # branch: 0x20-byte record whose +0x0c is the target sound index
                tone = dict(target=struct.unpack_from("<I", body, bank + data_off + arg + 0x0C)[0])
            elif typ == 7:  # plugin message: 0x20-byte 'DPMS' record, name at +0x08 (an SBI_ sample
                # name such as H_ATTKS1, a controller shake CSH_*, a music cue)
                rec = body[bank + data_off + arg:bank + data_off + arg + 0x20]
                tone = dict(msg=rec[8:16].split(bytes(1))[0].decode("latin-1"))
            grains.append((typ, arg, delay, tone))
        sounds.append(dict(index=s, name=names.get(s, f"sound_{s:03d}"), vol=vol, vol_group=vgrp, pan=pan,
                           instances=inst, flags=flags, grains=grains))
    return sounds, samples, ngrain


def grain_text(g, sounds):
    typ, arg, delay, extra = g
    t = GRAIN_TYPES.get(typ, f"op{typ:02x}")
    if typ == 1:
        s = f"tone({extra['offset']:#x},{tone_rate(extra['note'], extra['fine']):.0f}Hz)"
    elif typ == 7:
        s = f"plugin({extra['msg']})"
    elif typ == 8:
        tgt = extra["target"]
        s = f"branch({sounds[tgt]['name'] if tgt < len(sounds) else tgt})"
    else:
        s = f"{t}({arg:#x})"
    return s + (f"+{delay}" if delay else "")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("wad")
    ap.add_argument("record")
    ap.add_argument("out")
    a = ap.parse_args()
    body = next(b for _o, _t, _p, n, b in wad_records(open(a.wad, "rb").read()) if n == a.record and b)
    sounds, samples, ngrain = parse(body)
    os.makedirs(a.out, exist_ok=True)
    ntone = 0
    with open(os.path.join(a.out, "sounds.tsv"), "w", encoding="utf-8", newline="") as fh:
        fh.write("index\tname\tvol\tflags\tinstances\tprogram\n")
        for s in sounds:
            prog = " ".join(grain_text(g, sounds) for g in s["grains"])
            fh.write(f"{s['index']}\t{s['name']}\t{s['vol']}\t{s['flags']:#x}\t{s['instances']}\t{prog}\n")
            tones, seen = [], set()
            for _t, _a, _d, t in s["grains"]:
                if _t == 1:
                    key = (t["offset"], t["note"], t["fine"])
                    if key not in seen:
                        seen.add(key)
                        tones.append(t)
            for n, t in enumerate(tones):
                pcm = decode_psadpcm(samples[t["offset"]:t["offset"] + t["size"]])
                fn = s["name"] + (f"_{n + 1}" if len(tones) > 1 else "") + ".wav"
                with wave.open(os.path.join(a.out, fn), "wb") as w:
                    w.setnchannels(1)
                    w.setsampwidth(2)
                    w.setframerate(round(tone_rate(t["note"], t["fine"])))
                    w.writeframes(struct.pack(f"<{len(pcm)}h", *pcm))
                ntone += 1
    print(f"{a.record}: {len(sounds)} sounds, {ngrain} grains, {ntone} WAVs -> {a.out}")


if __name__ == "__main__":
    main()
