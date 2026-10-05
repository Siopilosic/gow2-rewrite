"""Write analysis/core_callgraph.dot: direct calls among core functions (from
analysis/core_functions.tsv) plus the virtual-call edges established in docs/confirmed.md
(dashed, labelled with the vtable slot)."""
import csv
import os

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))

# (caller, callee, label) - each verified in docs/confirmed.md (section in comment)
VIRTUAL = [
    ("Engine_UpdateServers", "Mgr_UpdateChildren", "table[5].vslot3"),           # C-B5
    ("Mgr_UpdateChildren", "Mgr_UpdateChild", "vslot7"),                         # C-C6
    ("Mgr_UpdateChildren", "Mgr_RemoveChildOnUpdate", "vslot8"),                 # C-C6
    ("Mgr_UpdateChild", "TextureServer_Update", "child.vslot3"),                 # C-C7
    ("Mgr_UpdateChild", "GOServer_Update", "child.vslot3"),                      # C-C7
    ("BootWad_DispatchRecord", "WadTag_Object", "handlers[1]"),                  # C-A5
    ("Wad_DispatchRecord", "WadTag_Object", "handlers[1]"),
    ("Wad_DispatchRecord", "WadTag_GroupStart", "handlers[2]"),
    ("Wad_DispatchRecord", "WadTag_GroupEnd", "handlers[3]"),
    ("Wad_DispatchRecord", "WadTag_ActivateByName", "handlers[5,0x13]"),
    ("WadTag_Object", "Root_CreateFromRecord", "table[0].vslot10"),              # C-C4
    ("WadTag_Object", "Mgr_CreateFromRecord", "table[5|0xd].vslot10"),
    ("WadTag_Object", "PooledServer_CreateFromRecord", "table[id].vslot10"),     # C-C7
    ("Root_CreateFromRecord", "Root_NewMasterServer", "vslot5->vslot19"),
    ("Root_CreateFromRecord", "Root_InitChild", "vslot22"),
    ("Root_InitChild", "Server_RegisterSelf", "child.vslot2"),
    ("Mgr_CreateFromRecord", "Mgr_Create", "vslot5"),
    ("Mgr_CreateFromRecord", "Mgr_AddChildSorted", "vslot6"),
    ("Mgr_Create", "Master_NewServer", "vslot19"),
    ("Mgr_Create", "RenMaster_NewServer", "vslot19"),
    ("Mgr_Create", "Mgr_InitChild", "vslot22"),
    ("Mgr_InitChild", "TextureServer_Init", "child.vslot2"),
    ("Mgr_InitChild", "GOServer_Init", "child.vslot2"),
    ("Mgr_InitChild", "Server_RegisterSelf", "child.vslot2"),
    ("TextureServer_Init", "TextureServer_NewDefaultBank", "vslot22"),
    ("GOServer_Init", "GOServer_NewDefaultBank", "vslot22"),
    ("PooledServer_CreateFromRecord", "PooledServer_SelectBank", "vslot19"),
    ("PooledServer_CreateFromRecord", "GOServer_SelectBank", "vslot19"),
    ("PooledServer_CreateFromRecord", "GOBank_NewDescriptor", "bank.vslot10"),  # C-D6
    ("PooledServer_CreateFromRecord", "GoClassA4_CreateFromRecord", "ctx.vslot10"),
    ("PooledServer_CreateFromRecord", "TextureBank_CreateFromRecord", "bank.vslot10"),  # C-D5
    ("PooledServer_CreateFromRecord", "MatBank_CreateFromRecord", "bank.vslot10"),  # C-D7
    ("TextureBank_CreateFromRecord", "TextureBank_NewTexture", "vslot20"),
    ("WadTag_ActivateByName", "PooledServer_Create", "owner.vslot5"),
    ("WadTag_ActivateByName", "GOServer_PushContext", "owner.vslot16"),
    ("WadTag_ActivateByName", "WadServer_Push", "owner.vslot16"),
    ("PooledServer_Create", "Bank_Create", "bank.vslot5"),
    ("Bank_Create", "GOBank_NewObject", "vslot19"),
    ("Bank_Create", "GOBank_OnCreated", "vslot22"),
    ("WadTag_GroupEnd", "PooledServer_GroupEnd", "owner.vslot11"),
    ("Engine_InitWadAndServers", "Boot_CreateServersAndEngineResources", "jal (hidden by Ghidra pass2)"),
]


def main():
    rows = list(csv.DictReader(open(os.path.join(ROOT, "analysis", "core_functions.tsv"), encoding="utf-8"),
                               delimiter="\t"))
    core = {r["name"] for r in rows}
    conf = {r["name"]: r["confidence"] for r in rows}
    out = ["digraph core {", '  rankdir=LR; node [shape=box, fontname="Consolas", fontsize=10];']
    colour = {"CONFIRMED": "palegreen", "HIGH": "lightblue", "MEDIUM": "khaki", "LOW": "lightgrey"}
    for n in sorted(core):
        out.append(f'  "{n}" [style=filled, fillcolor={colour.get(conf[n], "white")}];')
    edges = set()
    for r in rows:
        for c in filter(None, (x.strip() for x in r["callees"].split(","))):
            if c in core and (r["name"], c) not in edges:
                edges.add((r["name"], c))
                out.append(f'  "{r["name"]}" -> "{c}";')
    missing = [(a, b) for a, b, _ in VIRTUAL if a not in core or b not in core]
    if missing:
        print("virtual edges with an endpoint missing from core_functions.tsv:", missing)
    for a, b, lab in VIRTUAL:
        if a in core and b in core:
            out.append(f'  "{a}" -> "{b}" [style=dashed, label="{lab}", fontsize=8];')
    out.append("}")
    p = os.path.join(ROOT, "analysis", "core_callgraph.dot")
    open(p, "w", encoding="utf-8").write("\n".join(out) + "\n")
    print(f"{len(core)} nodes, {len(edges)} direct + {len(VIRTUAL)} virtual edges -> {p}")


if __name__ == "__main__":
    main()
