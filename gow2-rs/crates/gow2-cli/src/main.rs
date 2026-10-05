use std::collections::HashMap;
use std::error::Error;
use std::fs;
use std::path::Path;

use gow2_formats::{gfx, iso::Iso, toc, wad};
use gow2_model::{routing, servers as model, wad_tags};

type Res = Result<(), Box<dyn Error>>;

const USAGE: &str = "usage (analysis tools; nothing here runs game logic):
  gow2 ls <iso>                         list files on both disc layers
  gow2 toc <iso>                        list GODOFWAR.TOC entries
  gow2 extract <iso> <out-dir> [filter] extract PAK files whose name contains filter
  gow2 wad <file.wad>                   print the record tree with tag handler addresses
  gow2 route <file.wad>                 apply the recovered routing rules and check group semantics
  gow2 textures <file.wad> <out-dir>    export GFX/PAL pairs as PNG
  gow2 servers                          print recovered server registrations and classes";

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let a = |i: usize| args.get(i).map(String::as_str).unwrap_or_else(|| {
        eprintln!("{USAGE}");
        std::process::exit(2)
    });
    let r = match args.first().map(String::as_str) {
        Some("ls") => ls(a(1)),
        Some("toc") => toc_list(a(1)),
        Some("extract") => extract(a(1), a(2), args.get(3).map(String::as_str).unwrap_or("")),
        Some("wad") => wad_tree(a(1)),
        Some("route") => route(a(1)),
        Some("textures") => textures(a(1), a(2)),
        Some("servers") => servers(),
        _ => {
            eprintln!("{USAGE}");
            std::process::exit(2)
        }
    };
    if let Err(e) = r {
        eprintln!("error: {e}");
        std::process::exit(1);
    }
}

fn ls(iso: &str) -> Res {
    let mut iso = Iso::open(iso)?;
    for v in 0..iso.volumes.len() {
        println!("layer {v} (base sector {})", iso.volumes[v].base);
        for e in iso.walk(v)? {
            println!("  {:>12}  {}", if e.is_dir { "<DIR>".into() } else { e.size.to_string() }, e.path);
        }
    }
    Ok(())
}

fn read_toc(iso: &mut Iso) -> Result<Vec<toc::TocEntry>, Box<dyn Error>> {
    let (v, e) = iso.find("GODOFWAR.TOC")?;
    Ok(toc::parse_toc(&iso.read_file(v, &e)?))
}

fn toc_list(path: &str) -> Res {
    let mut iso = Iso::open(path)?;
    for e in read_toc(&mut iso)? {
        println!("{:<24} {:>10}  copies={:?}", e.name, e.size, e.copies);
    }
    Ok(())
}

fn extract(path: &str, out: &str, filter: &str) -> Res {
    let mut iso = Iso::open(path)?;
    let entries = read_toc(&mut iso)?;
    let mut pak = toc::Pak::open(iso)?;
    fs::create_dir_all(out)?;
    let filter = filter.to_ascii_uppercase();
    for e in entries.iter().filter(|e| e.name.to_ascii_uppercase().contains(&filter)) {
        fs::write(Path::new(out).join(&e.name), pak.read_entry(e)?)?;
        println!("{} ({} bytes)", e.name, e.size);
    }
    Ok(())
}

fn wad_tree(path: &str) -> Res {
    let data = fs::read(path)?;
    let mut depth = 0usize;
    for r in wad::records(&data) {
        if r.tag == wad::Tag::GroupEnd {
            depth = depth.saturating_sub(1);
        }
        let ty = r.obj_type().map(|t| format!(" type={:#010x} server={:?}", t.0, t.server())).unwrap_or_default();
        let raw_tag = u16::from_le_bytes([data[r.offset], data[r.offset + 1]]);
        let handler = wad_tags::handler(raw_tag).map(|h| format!(" handler={:#010x}", h.handler)).unwrap_or_default();
        println!("{:08x} {}{:?} p={} '{}' size={}{}{}", r.offset, "  ".repeat(depth), r.tag, r.param, r.name, r.size, ty, handler);
        if r.tag == wad::Tag::GroupStart {
            depth += 1;
        }
    }
    Ok(())
}

fn route(path: &str) -> Res {
    let data = fs::read(path)?;
    let rep = routing::route(&data);
    println!("object records per owner server (g_ServerTable[type & 0xffff]):");
    for (id, n) in &rep.objects_per_server {
        let name = model::registration(*id).map(|r| r.name).unwrap_or("?");
        println!("  {:<16} id={:#04x} records={}", name, id, n);
    }
    println!("instance records (bit 31, default bank): {}", rep.instance_records);
    println!("name-only object records (size 0): {}", rep.name_only_objects);
    println!("max group nesting: {} (stack depth {})", rep.max_group_depth, routing::GROUP_STACK_DEPTH);
    println!("GroupStart while pending: {}", rep.group_start_while_pending);
    println!("GroupStart not followed by Object: {}", rep.group_start_not_followed_by_object);
    println!("records for servers with no recovered class: {}", rep.unknown_servers.len());
    for (n, w) in rep.unknown_servers.iter().take(10) {
        println!("  {n} type={w:#010x}");
    }
    Ok(())
}

fn textures(path: &str, out: &str) -> Res {
    let data = fs::read(path)?;
    fs::create_dir_all(out)?;
    let mut gfxs = Vec::new();
    let mut pals = HashMap::new();
    for r in wad::records(&data) {
        if let Some(img) = gfx::parse(r.data).filter(|_| r.tag == wad::Tag::Object) {
            match r.name.strip_prefix("PAL_") {
                Some(k) => {
                    pals.insert(k.to_string(), img);
                }
                None => gfxs.push((r.name.trim_start_matches("GFX_").to_string(), img)),
            }
        }
    }
    let mut n = 0;
    for (key, img) in &gfxs {
        let (Some(pal), true) = (pals.get(key), img.bpp == 4 || img.bpp == 8) else { continue };
        let Some(rgba) = gfx::to_rgba(img, pal) else { continue };
        let f = fs::File::create(Path::new(out).join(format!("{key}.png")))?;
        let mut enc = png::Encoder::new(f, img.width, img.height);
        enc.set_color(png::ColorType::Rgba);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()?.write_image_data(&rgba)?;
        n += 1;
    }
    println!("wrote {n} textures to {out}");
    Ok(())
}

fn servers() -> Res {
    println!("Registrations from Boot_CreateServersAndEngineResources (0x00189b10), call order:");
    println!("  {:<16} {:>6} {:>4} {:>8} {:>8} {:>9}  class (factory size ctor vtable)", "name", "parent", "id", "arg_a3",
             "arg_t0", "order_key");
    for r in model::REGISTRATIONS {
        let c = model::class(r.id);
        let cls = c
            .map(|c| format!("{:#010x} {:#6x} {} {:#010x}{}", c.factory, c.size,
                             c.ctor.map(|a| format!("{a:#010x}")).unwrap_or_else(|| "inline    ".into()), c.vtable,
                             if c.pooled { " pooled" } else { "" }))
            .unwrap_or_else(|| "?".into());
        println!("  {:<16} {:>#6x} {:>#4x} {:>#8x} {:>#8x} {:>#9x}  {}", r.name, r.parent, r.id, r.arg_a3, r.arg_t0,
                 r.order_key, cls);
    }
    println!("\nMaster's per-frame child order (descending order_key; Mgr_UpdateChildren 0x00277200):");
    for r in model::master_update_order() {
        println!("  {:#06x} {}", r.order_key, r.name);
    }
    Ok(())
}
