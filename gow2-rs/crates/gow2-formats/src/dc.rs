//! The hero's gameplay data: the `DC_*` record family (WAD tags `0x0b` to `0x10`) and the move graph inside it.
//!
//! Layout and evidence: `docs/kratos-data.md` section 6 and `docs/combat.md`. In short:
//! * tag `0x0c` is a data blob. Pointers inside it are self-relative (target = field address + s32, 0 = null); lists are one u32
//!   with `count = w & 0xfff` and the array at `field + (s32 w >> 12)` holding self-relative pointers.
//! * tag `0x0f` names every hash the data uses (`hash = h * 127 + toupper(c)`), tag `0x10` labels every object with a name and type.
//! * a `MOV` (type `0x62`) names an animation clip by hash and carries three lists: branches (`+0x10`), hit windows (`+0x14`)
//!   and actions (`+0x18`). A branch (`tBranch`, `0x5f`, or `BRA`, `0x60`) points at its target `MOV`.
//! * `CRT_Hero + 0x18` is the list of entry branches the move system evaluates while the hero is in a locomotion move.
//!
//! Times in the data are IEEE half floats in normalised move time.

use std::collections::HashMap;

use crate::{fixed_str, le_u16, le_u32, wad::Record};

/// `FUN_00181428`: `h = h * 127 + toupper(c)` over signed bytes.
pub fn hash(s: &str, seed: u32) -> u32 {
    let mut h = seed;
    for &b in s.as_bytes() {
        let c = if b.is_ascii_lowercase() { b - 0x20 } else { b } as i8 as i32;
        h = h.wrapping_mul(127).wrapping_add(c as u32);
    }
    h
}

/// An IEEE half float.
pub fn half(h: u16) -> f32 {
    let sign = ((h & 0x8000) as u32) << 16;
    let exp = ((h >> 10) & 0x1f) as i32;
    let man = (h & 0x3ff) as u32;
    let bits = match exp {
        0 if man == 0 => sign,
        0 => {
            // subnormal: man * 2^-24
            let v = man as f32 * (1.0 / 16_777_216.0);
            return if sign != 0 { -v } else { v };
        }
        31 => sign | 0x7f80_0000 | (man << 13),
        _ => sign | (((exp + 112) as u32) << 23) | (man << 13),
    };
    f32::from_bits(bits)
}

/// Button codes 1 to 10 map to bits of the pad word (table at `0x002f6da7`). The bits follow the PS2 pad word.
pub const BUTTON_BIT: [i8; 11] = [-1, 6, 7, 4, 5, 2, 0, 9, 3, 1, 10];

#[derive(Debug, Clone, PartialEq)]
pub struct Branch {
    pub name: String,
    /// Index into `MoveSet::moves`, `None` for a branch without target (statistics and decision stubs).
    pub target: Option<usize>,
    pub flags_a: u32,
    pub flags_b: u32,
    /// Input window in normalised time of the current move.
    pub win: (f32, f32),
    /// Normalised time at which the target move starts.
    pub start_time: f32,
    pub tgt_health: (i16, i16),
    pub own_health: (i16, i16),
    pub b1e: i8,
    pub b1f: i8,
    /// Button code (1 to 10 are pad buttons, others are special triggers or events).
    pub button: u8,
    /// 1 pressed, 2 released, 3/4 held, 5 not held.
    pub press: u8,
    /// Stick condition (`docs/kratos-data.md` 6.4).
    pub stick: u8,
    /// Unlock or magic-selection requirement.
    pub unlock: u8,
    pub min_level: i8,
    /// `+0x04`: a context object compared with the caller's; null in all hero branches.
    pub has_context: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HitWindow {
    pub name: String,
    pub win: (f32, f32),
    pub ground: [f32; 3],
    pub air: [f32; 3],
    pub block: [f32; 3],
    pub damage: f32,
    /// Attack volume selector (`docs/combat.md` section 1).
    pub volume: u8,
    pub flags: u8,
    pub sub_hits: i8,
}

/// A concussion: the damage sphere a move spawns (`tActionConcussion` pointing at a `CNC_*` record, `docs/combat.md` section 13).
#[derive(Debug, Clone, PartialEq)]
pub struct Blast {
    pub name: String,
    /// Record byte 0: the shape kind (1 is a sphere that grows with time).
    pub shape: u8,
    /// The joint the sphere is placed at (`zeroJoint`, `synchJoint`, `linkJoint`, `pelvis`), found by its name hash.
    pub joint: String,
    /// The hit window carried by the record: damage, impulses, volume 8.
    pub hit: HitWindow,
    /// Seconds the blast lasts: the sum of the float list at `+0x1c`.
    pub duration: f32,
    /// The float list at `+0x28` (start and end values in metres).
    pub keys: Vec<f32>,
}

#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub name: String,
    pub kind: u8,
    pub flags: u8,
    pub trigger: u8,
    pub min_level: i8,
    pub win: (f32, f32),
    /// The first 0x20 bytes of the object, for the kind-specific fields.
    pub raw: [u8; 0x20],
    /// Set for `SCR_Concussion` actions.
    pub blast: Option<Blast>,
}

impl Action {
    pub fn f32_at(&self, off: usize) -> f32 {
        f32::from_bits(u32::from_le_bytes([self.raw[off], self.raw[off + 1], self.raw[off + 2], self.raw[off + 3]]))
    }
    pub fn half_at(&self, off: usize) -> f32 {
        half(u16::from_le_bytes([self.raw[off], self.raw[off + 1]]))
    }
    pub fn u32_at(&self, off: usize) -> u32 {
        u32::from_le_bytes([self.raw[off], self.raw[off + 1], self.raw[off + 2], self.raw[off + 3]])
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct Move {
    pub name: String,
    pub off: usize,
    /// `+0x00`: playback rate (half), mostly 1.0.
    pub rate: f32,
    /// `+0x02`: blend-in time in seconds (half), MEDIUM.
    pub blend: f32,
    /// `+0x04`: flags; bit 3 (`& 8`) marks a move that layers over others (locomotion), bits 8 to 10 are the move class.
    pub flags: u32,
    pub anim_hash: u32,
    /// The clip name for `anim_hash`, when the name table has it.
    pub anim: String,
    pub branches: Vec<Branch>,
    pub hits: Vec<HitWindow>,
    pub actions: Vec<Action>,
}

#[derive(Debug, Default, Clone)]
pub struct MoveSet {
    pub moves: Vec<Move>,
    pub by_name: HashMap<String, Vec<usize>>,
    /// `CRT_Hero + 0x18`: the entry branches.
    pub entry: Vec<Branch>,
}

impl MoveSet {
    /// The first move with this name.
    pub fn find(&self, name: &str) -> Option<usize> {
        self.by_name.get(name).and_then(|v| v.first().copied())
    }
}

struct Blob<'a> {
    b: &'a [u8],
}

impl Blob<'_> {
    fn i32(&self, o: usize) -> i32 {
        le_u32(self.b, o) as i32
    }
    fn rel(&self, o: usize) -> Option<usize> {
        let v = self.i32(o);
        (v != 0).then(|| (o as i64 + v as i64) as usize)
    }
    /// A packed list: the self-relative pointers of its array.
    fn list(&self, o: usize) -> Vec<Option<usize>> {
        let w = self.i32(o);
        let n = (w & 0xfff) as usize;
        if n == 0 {
            return Vec::new();
        }
        let arr = (o as i64 + (w >> 12) as i64) as usize;
        (0..n).map(|k| self.rel(arr + 4 * k)).collect()
    }
    fn half(&self, o: usize) -> f32 {
        half(le_u16(self.b, o))
    }
    /// A packed list of f32 values.
    fn floats(&self, o: usize) -> Vec<f32> {
        let w = self.i32(o);
        let n = (w & 0xfff) as usize;
        if n == 0 {
            return Vec::new();
        }
        let arr = (o as i64 + (w >> 12) as i64) as usize;
        (0..n).map(|k| f32::from_bits(le_u32(self.b, arr + 4 * k))).collect()
    }
}

/// A weapon attachment record (`tChained`, `tHand`; `docs/animation.md`, "Blade attachment"): the joint names of the three slots.
#[derive(Debug, Clone, PartialEq)]
pub struct Attachment {
    /// The object it carries (`goMAIBlade`).
    pub object: String,
    /// Slot 1: the hand joint (`LWeapIH`).
    pub hand: String,
    /// Slot 2: the free joint the clips animate (`LWeapOH`).
    pub free: String,
    /// Slot 0: stowed on the back (`LeftBladeBack`).
    pub stowed: String,
    /// Snap distance in metres (`+0x10`).
    pub snap_m: f32,
    /// `+0x20`: 1 = chained blade, 0 = hand-held.
    pub chained: bool,
    /// Chained only: the chain joints `LChain`, `LChainW`.
    pub chain: [String; 2],
}

/// The DC container of a hero WAD, decoded into moves, branches, hit windows and actions.
pub struct Dc {
    pub blob: Vec<u8>,
    /// Hash to name, from tag `0x0f`.
    pub names: HashMap<u32, String>,
    /// `(offset, name, type)` from tag `0x10`, sorted by offset.
    pub objects: Vec<(usize, String, u32)>,
}

const TYPE_MOV: u32 = 0x62;
const TYPE_TBRANCH: u32 = 0x5f;

impl Dc {
    /// Finds the DC container with the largest data blob among the WAD's records.
    pub fn from_records(recs: &[Record]) -> Option<Dc> {
        let mut groups: HashMap<&str, HashMap<u16, &[u8]>> = HashMap::new();
        for r in recs {
            if let crate::wad::Tag::Other(t) = r.tag {
                if (0x0b..=0x10).contains(&t) {
                    groups.entry(r.name.as_str()).or_default().insert(t, r.data);
                }
            }
        }
        let g = groups.into_values().max_by_key(|g| g.get(&0x0c).map_or(0, |b| b.len()))?;
        let blob = g.get(&0x0c)?.to_vec();
        let mut names = HashMap::new();
        if let Some(p) = g.get(&0x0f) {
            let n = le_u32(p, 0) as usize;
            for i in 0..n {
                let (h, o) = (le_u32(p, 4 + 8 * i), le_u32(p, 8 + 8 * i) as usize);
                names.insert(h, fixed_str(&p[o..]));
            }
        }
        let mut objects = Vec::new();
        if let Some(p) = g.get(&0x10) {
            let n = le_u32(p, 0) as usize;
            for i in 0..n {
                let (off, no, ty) = (le_u32(p, 4 + 12 * i) as usize, le_u32(p, 8 + 12 * i) as usize, le_u32(p, 12 + 12 * i));
                objects.push((off, fixed_str(&p[no..]), ty));
            }
        }
        objects.sort();
        Some(Dc { blob, names, objects })
    }

    fn name_at(&self, off: usize) -> String {
        match self.objects.binary_search_by_key(&off, |o| o.0) {
            Ok(i) => self.objects[i].1.clone(),
            Err(_) => format!("blob+{off:x}"),
        }
    }

    /// The attachment records an `ATT_*` object points to (two pointers for `ATT_Chains`, one for the others).
    pub fn attachments(&self, att: &str, count: usize) -> Vec<Attachment> {
        let b = Blob { b: &self.blob };
        let Some(&(off, _, _)) = self.objects.iter().find(|o| o.1 == att) else { return Vec::new() };
        let text = |o: usize| -> String {
            b.rel(o).filter(|&p| p < self.blob.len()).map(|p| fixed_str(&self.blob[p..])).unwrap_or_default()
        };
        (0..count)
            .filter_map(|i| b.rel(off + 4 * i))
            .map(|r| Attachment {
                object: text(r),
                hand: text(r + 4),
                free: text(r + 8),
                stowed: text(r + 12),
                snap_m: f32::from_bits(le_u32(&self.blob, r + 0x10)),
                chained: self.blob[r + 0x20] == 1,
                // only `tChained` records have the chain joints: for a `tHand` the bytes there belong to the next record
                chain: if self.blob[r + 0x20] == 1 { [text(r + 0x24), text(r + 0x28)] } else { [String::new(), String::new()] },
            })
            .collect()
    }

    /// The two chained blades (`ATT_Chains`): left first, then right.
    pub fn chained_attachments(&self) -> Vec<Attachment> {
        self.attachments("ATT_Chains", 2)
    }

    /// Decodes every move, and the entry branches of `CRT_Hero`.
    pub fn move_set(&self) -> MoveSet {
        let b = Blob { b: &self.blob };
        let mov_offs: Vec<usize> = self.objects.iter().filter(|o| o.2 == TYPE_MOV).map(|o| o.0).collect();
        let index: HashMap<usize, usize> = mov_offs.iter().enumerate().map(|(i, &o)| (o, i)).collect();
        let branch = |o: usize| -> Branch {
            let s16 = |k: usize| i16::from_le_bytes([self.blob[o + k], self.blob[o + k + 1]]);
            Branch {
                name: self.name_at(o),
                target: b.rel(o).and_then(|t| index.get(&t).copied()),
                flags_a: le_u32(&self.blob, o + 8),
                flags_b: le_u32(&self.blob, o + 12),
                win: (b.half(o + 0x10), b.half(o + 0x12)),
                start_time: b.half(o + 0x14),
                tgt_health: (s16(0x16), s16(0x18)),
                own_health: (s16(0x1a), s16(0x1c)),
                b1e: self.blob[o + 0x1e] as i8,
                b1f: self.blob[o + 0x1f] as i8,
                button: self.blob[o + 0x20],
                press: self.blob[o + 0x21],
                stick: self.blob[o + 0x22],
                unlock: self.blob[o + 0x23],
                min_level: self.blob[o + 0x24] as i8,
                has_context: b.rel(o + 4).is_some(),
            }
        };
        let vec3 = |o: usize| [b.half(o), b.half(o + 2), b.half(o + 4)];
        let window = |c: usize| HitWindow {
            name: self.name_at(c),
            win: (b.half(c), b.half(c + 2)),
            ground: vec3(c + 4),
            air: vec3(c + 0xa),
            block: vec3(c + 0x10),
            damage: b.half(c + 0x16),
            volume: self.blob[c + 0x18],
            flags: self.blob[c + 0x19],
            sub_hits: self.blob[c + 0x1a] as i8,
        };
        let joint_names: HashMap<u32, &str> = [("zeroJoint"), ("synchJoint"), ("linkJoint"), ("pelvis")].into_iter().map(|n| (hash(n, 0), n)).collect();
        let blast = |a: usize| -> Option<Blast> {
            // the action's script hash names SCR_Concussion; its pointer at +0x0c leads to the CNC record
            if self.blob[a] != 0x0d || self.names.get(&le_u32(&self.blob, a + 8)).map(String::as_str) != Some("SCR_Concussion") {
                return None;
            }
            let r = b.rel(a + 0x0c)?;
            let hit = b.rel(r + 0x0c)?;
            Some(Blast {
                name: self.name_at(r),
                shape: self.blob[r],
                joint: joint_names.get(&le_u32(&self.blob, r + 8)).map_or(String::new(), |s| s.to_string()),
                hit: window(hit),
                duration: b.floats(r + 0x1c).iter().sum(),
                keys: b.floats(r + 0x28),
            })
        };
        let mut set = MoveSet::default();
        for &o in &mov_offs {
            let anim_hash = le_u32(&self.blob, o + 8);
            let mv = Move {
                name: self.name_at(o),
                off: o,
                rate: b.half(o),
                blend: b.half(o + 2),
                flags: le_u32(&self.blob, o + 4),
                anim_hash,
                anim: self.names.get(&anim_hash).cloned().unwrap_or_default(),
                branches: b.list(o + 0x10).into_iter().flatten().map(&branch).collect(),
                hits: b
                    .list(o + 0x14)
                    .into_iter()
                    .flatten()
                    .map(|c| HitWindow {
                        name: self.name_at(c),
                        win: (b.half(c), b.half(c + 2)),
                        ground: vec3(c + 4),
                        air: vec3(c + 0xa),
                        block: vec3(c + 0x10),
                        damage: b.half(c + 0x16),
                        volume: self.blob[c + 0x18],
                        flags: self.blob[c + 0x19],
                        sub_hits: self.blob[c + 0x1a] as i8,
                    })
                    .collect(),
                actions: b
                    .list(o + 0x18)
                    .into_iter()
                    .flatten()
                    .map(|a| {
                        let mut raw = [0u8; 0x20];
                        let end = (a + 0x20).min(self.blob.len());
                        raw[..end - a].copy_from_slice(&self.blob[a..end]);
                        Action {
                            name: self.name_at(a),
                            kind: self.blob[a],
                            flags: self.blob[a + 1],
                            trigger: self.blob[a + 2],
                            min_level: self.blob[a + 3] as i8,
                            win: (b.half(a + 4), b.half(a + 6)),
                            raw,
                            blast: blast(a),
                        }
                    })
                    .collect(),
            };
            set.by_name.entry(mv.name.clone()).or_default().push(set.moves.len());
            set.moves.push(mv);
        }
        if let Some(crt) = self.objects.iter().find(|o| o.1 == "CRT_Hero") {
            set.entry = b
                .list(crt.0 + 0x18)
                .into_iter()
                .flatten()
                .filter(|&o| matches!(self.objects.binary_search_by_key(&o, |x| x.0), Ok(i) if self.objects[i].2 == TYPE_TBRANCH || self.objects[i].2 == 0x60))
                .map(&branch)
                .collect();
        }
        set
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn halves_decode() {
        assert_eq!(half(0x3c00), 1.0);
        assert_eq!(half(0x0000), 0.0);
        assert_eq!(half(0xc000), -2.0);
        assert!((half(0x3266) - 0.2).abs() < 1e-3);
        assert_eq!(half(0x7c00), f32::INFINITY);
    }

    #[test]
    fn the_name_hash_matches_the_game() {
        // reproduced by tools/dcparse.py; "MOV_Stand" in the hero's table
        assert_eq!(hash("A", 0), 65);
        assert_eq!(hash("ab", 0), 65 * 127 + 66);
        assert_eq!(hash("ab", 0), hash("AB", 0));
    }
}

