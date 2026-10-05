"""Dump Kratos's tuning block and speed profile from a RAM capture.

  python tools/kratos_tuning.py [--capture ingame2] [--out analysis/kratos_tuning.tsv]

The Character object of Kratos is at 0x00958160 in the captures (docs/runtime-validation.md). Its tuning
block is the tSoldier object at Character+0x3c0 (docs/kratos-data.md 6.8). The speed profile comes from
Character virtual +0x124 (FUN_0022cb10, docs/character-update.md 6.4). Speeds in the block are in metres
per second; the code multiplies them by 16 (world units per metre).
The `use` column lists the readings documented so far; an empty use means the field is not read yet.
"""
import argparse
import os
import struct

ROOT = os.path.join(os.path.dirname(__file__), "..")
KRATOS = 0x00958160
SIZE = 0x240

USES = {
    0x24: "capsule size (FUN_002233d0)", 0x28: "capsule size (FUN_002233d0)",
    0x30: "capsule size (FUN_002233d0)", 0x34: "capsule size (FUN_002233d0)",
    0x38: "terminal fall speed, m/s (character-update 6.2, 8)",
    0x3c: "high-fall threshold: vy < -value (character-update 8)",
    0x40: "air acceleration, m/s^2 (character-update 6.2)",
    0x74: "stick scale when +0x374 & 2 (character-update 6.2)",
    0x78: "jump launch speed, m/s (character-update 8)",
    0x7c: "double-jump launch speed, m/s (character-update 8)",
    0x80: "double-jump window: vy < value (character-update 8)",
    0x84: "double-jump window: vy > value (character-update 8)",
    0x88: "rise-to-fall switch: vy < value (character-update 8)",
    0x8c: "air-control factor (character-update 6.2)",
    0x98: "gravity factor in state 0x1000 (character-update 6.2)",
    0xc0: "ledge hang lateral speed, m/s (character-update 10)",
    0xc4: "ladder speed, m/s (character-update 10)",
    0xc8: "free climb minimum speed, m/s (character-update 10)",
    0xd0: "free climb maximum speed, m/s (character-update 10)",
    0xe4: "wall slide: climb speed / slide exit, m/s (character-update 10)",
    0xe8: "wall slide: stick-down threshold (character-update 10)",
    0xec: "wall slide: maximum slide speed, m/s (character-update 10)",
    0xf0: "wall slide: slide acceleration, m/s^2 (character-update 10)",
    0xf4: "wall slide: braking, m/s^2 (character-update 10)",
    0xcc: "wall jump: lateral stick limit (x 0.5) (character-update 9)",
    0xd8: "wall kick-off: upward launch speed, m/s (character-update 9)",
    0xdc: "wall kick-off push-away speed; wall-normal climb speed, m/s (character-update 9, 10)",
    0xf8: "rope climb rate curve, second argument (character-update 12)",
    0xfc: "rope climb rate curve, stick scale (character-update 12)",
    0x10c: "rope maximum slide speed, m/s (character-update 12)",
    0x110: "rope slide acceleration, m/s^2 (character-update 12)",
    0x114: "rope slide braking, m/s^2 (character-update 12)",
    0x118: "hand-over-hand rate curve, second argument (character-update 12)",
    0x11c: "hand-over-hand rate curve, stick scale (character-update 12)",
    0x180: "swim minimum speed, m/s (character-update 11)",
    0x184: "swim speed, m/s (character-update 11)",
    0x188: "surface swim speed, m/s (character-update 11)",
    0x18c: "swim acceleration, m/s^2 (character-update 11)",
    0x190: "swim deceleration, m/s^2 (character-update 11)",
    0x194: "swim turn/roll factor (character-update 11)",
    0x198: "surface pull rate (character-update 11)",
    0x1f8: "grapple swing pump factor, pendulum points (character-update 13)",
    0x200: "grapple swing factor, circular points (character-update 13)",
    0x20c: "grapple: read at entry, meaning open (character-update 13)",
    0x210: "grapple minimum line length, m (character-update 13)",
    0x214: "grapple maximum line length, m (character-update 13)",
    0x1b8: "speed profile override when +0x174 & 0x80: min", 0x1bc: "override: target",
    0x1c0: "override: acceleration", 0x1c4: "override: deceleration", 0x1c8: "override: turn rate",
    0x1d0: "ceiling-hang profile: min", 0x1d4: "ceiling-hang profile: target",
    0x1d8: "ceiling-hang profile: acceleration", 0x1dc: "ceiling-hang profile: deceleration",
    0x1e0: "ceiling-hang profile: turn rate",
}
PROFILE = ["minimum moving speed, m/s", "target (full-stick) speed, m/s", "acceleration, m/s^2",
           "deceleration, m/s^2", "turn rate per 1/60 s", "", "", "", ""]


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--capture", default="ingame2")
    ap.add_argument("--out", default=os.path.join(ROOT, "analysis", "kratos_tuning.tsv"))
    a = ap.parse_args()
    ram = open(os.path.join(ROOT, "analysis", "runtime", a.capture, "ee.bin"), "rb").read()
    u32 = lambda addr: struct.unpack_from("<I", ram, addr & 0x1FFFFFF)[0]
    s32 = lambda addr: struct.unpack_from("<i", ram, addr & 0x1FFFFFF)[0]
    f32 = lambda addr: struct.unpack_from("<f", ram, addr & 0x1FFFFFF)[0]
    tuning = u32(KRATOS + 0x3C0)
    rows = [("tuning", f"+0x{o:03x}", f"{f32(tuning + o):.6g}", f"{u32(tuning + o):08x}", USES.get(o, ""))
            for o in range(0, SIZE, 4)]
    crt, anim = u32(KRATOS + 8), u32(KRATOS + 0x3C4)
    rel = s32(crt + 0x1C)
    base = crt + 0x1C + (rel >> 12)
    stack = ram[(anim + 0xEA) & 0x1FFFFFF]
    entry = base + stack * 4
    entry += s32(entry)
    profile = entry + 4 + s32(entry + 4)
    rows += [("profile", f"[{i}]", f"{f32(profile + 4 * i):.6g}", f"{u32(profile + 4 * i):08x}", use)
             for i, use in enumerate(PROFILE)]
    with open(a.out, "w", encoding="utf-8") as fh:
        fh.write(f"#block\toffset\tvalue\traw\tuse (capture {a.capture}, tuning 0x{tuning:08x}, "
                 f"profile 0x{profile:08x})\n")
        for r in rows:
            fh.write("\t".join(r) + "\n")
    print(f"tuning 0x{tuning:08x}, profile 0x{profile:08x}, {len(rows)} rows,",
          sum(1 for r in rows if r[4]), "with a documented use")


if __name__ == "__main__":
    main()
