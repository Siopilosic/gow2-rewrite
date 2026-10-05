"""Feature specification database (SQLite3).

One row per behaviour rule of the original game, with an evidence level.
The database file analysis/features.db is the writable authority for feature rows.
The model files (symbols.tsv, structs.txt, sigs.tsv, volatile.tsv) stay authoritative for
names, types and signatures. This tool never touches them.

Usage:
  python tools/features_db.py init     create the database and seed rows (does nothing if it exists)
  python tools/features_db.py show     print all rows
  python tools/features_db.py gaps     print rows that are UNKNOWN, LOW or NOT TESTED

Levels: CONFIRMED, HIGH, MEDIUM, LOW, SPECULATIVE, UNKNOWN, UNSTATED (the doc does not give a level).
Status: open, awaiting human test, tested pass, tested fail, not tested.
Seed rows only repeat what the cited doc already says. Nothing here is new evidence.
"""
import sqlite3
import sys
from pathlib import Path

DB = Path(__file__).resolve().parent.parent / "analysis" / "features.db"

SCHEMA = """
CREATE TABLE features (
  id TEXT PRIMARY KEY,
  title TEXT NOT NULL,
  reference TEXT NOT NULL
);
CREATE TABLE rules (
  id TEXT PRIMARY KEY,
  feature_id TEXT NOT NULL REFERENCES features(id),
  rule TEXT NOT NULL,
  level TEXT NOT NULL CHECK (level IN
    ('CONFIRMED','HIGH','MEDIUM','LOW','SPECULATIVE','UNKNOWN','UNSTATED')),
  evidence TEXT NOT NULL,
  missing TEXT NOT NULL DEFAULT '',
  status TEXT NOT NULL DEFAULT 'open'
);
CREATE TABLE feedback (
  id INTEGER PRIMARY KEY AUTOINCREMENT,
  feature_id TEXT NOT NULL REFERENCES features(id),
  build TEXT NOT NULL,
  tester TEXT NOT NULL,
  steps TEXT NOT NULL,
  expected TEXT NOT NULL,
  actual TEXT NOT NULL,
  result TEXT NOT NULL CHECK (result IN ('pass','fail','not tested')),
  recorded TEXT NOT NULL DEFAULT (date('now'))
);
"""

REF = "God of War II NTSC-U v1.01 SCUS_974.81"
F = "kratos.ground.locomotion"

RULES = [
    ("handler", "Ground locomotion handler is FUN_002284c8, chosen when no special state is set.",
     "HIGH", "docs/character-update.md 3.1 and 6", ""),
    ("writer", "While walking, the hero position has one writer: Character_Update (vtable 0x2f1440 slot 2) to Node_SetMatrix.",
     "CONFIRMED", "docs/research-status.md Phase 3 (runtime write-watch)", ""),
    ("speed", "Speed profile: minimum 0.9, target 7.5 m/s, acceleration and deceleration 50 m/s^2, 16 units per metre.",
     "CONFIRMED", "docs/rust-port.md (read from RAM)", ""),
    ("turn", "Turn is an exponential approach: heading += wrapped angle x 0.25 x dt x 60 (FUN_00221be0).",
     "UNSTATED", "docs/rust-port.md (ported from decompilation)", "level not stated in the doc"),
    ("tick", "Gameplay runs on a fixed 59.94 Hz tick.",
     "UNSTATED", "docs/project-goal.md", "level not stated in the doc"),
    ("friction", "Ground friction coefficient comes from the ground collider material.",
     "UNKNOWN", "docs/rust-port.md says it is not decoded; Rust uses a default tuned to a 0.15 s stop (LOW)",
     "decode of the collider material"),
    ("walkblend", "Walk animation is a speed mix of navWalkSlow and navWalkFast.",
     "MEDIUM", "docs/rust-port.md", "the exact walkBlend node"),
    ("fade", "Fade times between idle and walk.",
     "LOW", "docs/rust-port.md (guessed)", "runtime evidence"),
    ("steps", "Step height: up 10 units, down 14 units.",
     "LOW", "docs/rust-port.md", "evidence from the game's move loop"),
    ("capsule", "Body radius 0.6 m, height 2.2 m.",
     "MEDIUM", "docs/rust-port.md (from the tuning)", "confirmation against the capsule"),
    ("ledge", "Ledge, ladder and climb states, camera zones, coyote time.",
     "UNKNOWN", "docs/rust-port.md lists them as not yet ported", "a ledge capture and handler reading"),
]

def init():
    if DB.exists():
        print(f"{DB} already exists, nothing changed.")
        return
    con = sqlite3.connect(DB)
    con.executescript(SCHEMA)
    con.execute("INSERT INTO features VALUES (?,?,?)", (F, "Kratos ground locomotion", REF))
    for rid, rule, level, ev, missing in RULES:
        con.execute(
            "INSERT INTO rules (id, feature_id, rule, level, evidence, missing, status) VALUES (?,?,?,?,?,?,?)",
            (f"{F}.{rid}", F, rule, level, ev, missing, "open"))
    con.execute(
        "INSERT INTO rules (id, feature_id, rule, level, evidence, missing, status) VALUES (?,?,?,?,?,?,?)",
        (f"{F}.playtest", F, "Rust build matches the original when walking in the same level.",
         "UNKNOWN", "none", "human playtest in RHOD10 against PCSX2", "not tested"))
    con.commit()
    con.close()
    print(f"Created {DB} with {len(RULES) + 1} rows.")

def rows(where=""):
    con = sqlite3.connect(DB)
    q = "SELECT id, level, status, rule, evidence, missing FROM rules " + where + " ORDER BY id"
    for r in con.execute(q):
        print(f"{r[0]}\n  level={r[1]} status={r[2]}\n  rule: {r[3]}\n  evidence: {r[4]}\n  missing: {r[5] or '-'}")
    con.close()

if __name__ == "__main__":
    cmd = sys.argv[1] if len(sys.argv) > 1 else "show"
    if cmd == "init":
        init()
    elif cmd == "show":
        rows()
    elif cmd == "gaps":
        rows("WHERE level IN ('UNKNOWN','LOW','SPECULATIVE') OR status='not tested'")
    else:
        print(__doc__)
