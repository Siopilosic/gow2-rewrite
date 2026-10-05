// Applies the reverse-engineering model to the program:
//   1. clears wrongly-inferred "no return" flags (function body contains `jr ra`)
//   2. creates functions at every vtable slot target from analysis/vtables.tsv and at every missed
//      start in analysis/missed_functions.tsv (address built by lui+addiu, preceded by jr ra)
//   3. creates structures from analysis/structs.txt (category /GoW2)
//   4. applies names from analysis/symbols.tsv (confirmed/high -> rename; medium/low -> comment only)
//   5. applies data types to globals (symbols.tsv column 6) and signatures from analysis/sigs.tsv
// Usage: -postScript ApplyCore.java <analysis-dir>
//@category GoW2
import java.io.*;
import java.util.*;

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.data.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.listing.Function.FunctionUpdateType;
import ghidra.program.model.symbol.*;
import ghidra.program.model.util.CodeUnitInsertionException;

public class ApplyCore extends GhidraScript {

	private DataTypeManager dtm;
	private final CategoryPath cat = new CategoryPath("/GoW2");
	private final Map<String, Structure> structs = new LinkedHashMap<>();
	private PrintWriter log;

	@Override
	protected void run() throws Exception {
		File dir = new File(getScriptArgs().length > 0 ? getScriptArgs()[0] : "analysis");
		dtm = currentProgram.getDataTypeManager();
		log = new PrintWriter(new FileWriter(new File(dir, "apply_core.log")));
		try {
			markVolatile(new File(dir, "volatile.tsv"));
			fixNoReturn();
			createVtableFunctions(new File(dir, "vtables.tsv"));
			createMissedFunctions(new File(dir, "missed_functions.tsv"));
			loadStructs(new File(dir, "structs.txt"));
			applySymbols(new File(dir, "symbols.tsv"));
			applySigs(new File(dir, "sigs.tsv"));
		}
		finally {
			log.close();
		}
	}

	private void say(String s) {
		println(s);
		log.println(s);
	}

	// ---------------------------------------------------------------- 0
	// Splits the containing memory block so [addr, addr+size) is its own block, then marks it volatile.
	private void markVolatile(File f) throws Exception {
		if (!f.exists()) {
			return;
		}
		ghidra.program.model.mem.Memory mem = currentProgram.getMemory();
		int n = 0;
		try (BufferedReader r = new BufferedReader(new FileReader(f))) {
			String line;
			while ((line = r.readLine()) != null) {
				if (line.isBlank() || line.startsWith("#")) {
					continue;
				}
				String[] c = line.split("\t");
				Address a = toAddr(Long.parseLong(c[0], 16));
				Address end = a.add(Integer.parseInt(c[1]));
				ghidra.program.model.mem.MemoryBlock b = mem.getBlock(a);
				if (b == null) {
					say("volatile: no block at " + a);
					continue;
				}
				if (b.isVolatile() && b.getStart().equals(a) && b.getEnd().equals(end.subtract(1))) {
					continue;
				}
				if (!b.getStart().equals(a)) {
					mem.split(b, a);
				}
				b = mem.getBlock(a);
				if (b.getEnd().compareTo(end) >= 0) {
					mem.split(b, end);
				}
				b = mem.getBlock(a);
				b.setName("vol_" + c[0]);
				b.setVolatile(true);
				n++;
			}
		}
		say("volatile blocks created: " + n);
	}

	// ---------------------------------------------------------------- 1
	private void fixNoReturn() {
		int fixed = 0;
		for (Function f : currentProgram.getFunctionManager().getFunctions(true)) {
			if (!f.hasNoReturn()) {
				continue;
			}
			boolean returns = false;
			for (Instruction ins : currentProgram.getListing().getInstructions(f.getBody(), true)) {
				if (ins.getMnemonicString().equals("jr") && ins.toString().contains("ra")) {
					returns = true;
					break;
				}
			}
			if (returns) {
				f.setNoReturn(false);
				fixed++;
				log.println("noreturn cleared: " + f.getName() + " @ " + f.getEntryPoint());
			}
		}
		say("no-return flags cleared (body contains jr ra): " + fixed);
	}

	// ---------------------------------------------------------------- 2
	private void createVtableFunctions(File f) throws Exception {
		int made = 0, total = 0;
		try (BufferedReader r = new BufferedReader(new FileReader(f))) {
			String line = r.readLine(); // header
			while ((line = r.readLine()) != null) {
				line = line.replace("﻿", "");
				String[] c = line.split("\t");
				if (c.length < 5 || c[4].isEmpty()) {
					continue;
				}
				for (String fn : c[4].split(",")) {
					long v = Long.parseLong(fn, 16);
					if (v == 0 || v == 0x2bff80) {
						continue;
					}
					total++;
					Address a = toAddr(v);
					if (getFunctionAt(a) == null) {
						disassemble(a);
						if (createFunction(a, null) != null) {
							made++;
						}
					}
				}
			}
		}
		say("vtable slot targets: " + total + ", functions created: " + made);
	}

	// Function starts that auto-analysis missed (tools/missed_functions.py).
	private void createMissedFunctions(File f) throws Exception {
		if (!f.exists()) {
			return;
		}
		int made = 0, total = 0;
		try (BufferedReader r = new BufferedReader(new FileReader(f))) {
			String line;
			while ((line = r.readLine()) != null) {
				if (line.isBlank() || line.startsWith("#")) {
					continue;
				}
				total++;
				Address a = toAddr(Long.parseLong(line.split("	")[0], 16));
				if (getFunctionAt(a) == null) {
					disassemble(a);
					if (createFunction(a, null) != null) {
						made++;
					}
				}
			}
		}
		say("missed function starts: " + total + ", functions created: " + made);
	}

	// ---------------------------------------------------------------- 3
	private DataType parseType(String t) {
		if (t.endsWith("*")) {
			return new PointerDataType(parseType(t.substring(0, t.length() - 1)), dtm);
		}
		if (t.startsWith("ptr:")) {
			return new PointerDataType(parseType(t.substring(4)), dtm);
		}
		int br = t.indexOf('[');
		if (br > 0) {
			int n = Integer.parseInt(t.substring(br + 1, t.length() - 1));
			DataType e = parseType(t.substring(0, br));
			return new ArrayDataType(e, n, e.getLength(), dtm);
		}
		switch (t) {
			case "u8": return ByteDataType.dataType;
			case "s8": return SignedByteDataType.dataType;
			case "u16": return UnsignedShortDataType.dataType;
			case "s16": return ShortDataType.dataType;
			case "u32": return UnsignedIntegerDataType.dataType;
			case "s32": return IntegerDataType.dataType;
			case "u64": return UnsignedLongLongDataType.dataType;
			case "f32": return FloatDataType.dataType;
			case "char": return CharDataType.dataType;
			case "ptr": return PointerDataType.dataType;
			case "void": return VoidDataType.dataType;
		}
		Structure s = structs.get(t);
		if (s == null) {
			throw new IllegalArgumentException("unknown type " + t);
		}
		return s;
	}

	private void loadStructs(File f) throws Exception {
		Structure cur = null;
		int fields = 0;
		try (BufferedReader r = new BufferedReader(new FileReader(f))) {
			String line;
			while ((line = r.readLine()) != null) {
				String body = line.trim();
				if (body.isEmpty() || body.startsWith("#")) {
					continue;
				}
				String comment = null;
				int sc = body.indexOf(';');
				if (sc >= 0) {
					comment = body.substring(sc + 1).trim();
					body = body.substring(0, sc).trim();
				}
				String[] c = body.split("\\s+");
				if (c[0].equals("struct")) {
					int size = Integer.parseInt(c[2].replace("0x", ""), 16);
					StructureDataType s = new StructureDataType(cat, c[1], size, dtm);
					cur = (Structure) dtm.addDataType(s, DataTypeConflictHandler.REPLACE_HANDLER);
					structs.put(c[1], cur);
					continue;
				}
				int off = Integer.parseInt(c[0], 16);
				DataType dt = parseType(c[1]);
				cur.replaceAtOffset(off, dt, dt.getLength(), c[2], comment);
				fields++;
			}
		}
		say("structures: " + structs.size() + ", fields: " + fields);
	}

	// ---------------------------------------------------------------- 4/5
	private void applySymbols(File f) throws Exception {
		int renamed = 0, proposed = 0, data = 0;
		try (BufferedReader r = new BufferedReader(new FileReader(f))) {
			String line;
			while ((line = r.readLine()) != null) {
				if (line.isBlank() || line.startsWith("#")) {
					continue;
				}
				String[] c = line.split("\t");
				Address a = toAddr(Long.parseLong(c[0], 16));
				String kind = c[1], name = c[2], conf = c[3], ev = c.length > 4 ? c[4] : "";
				boolean strong = conf.equals("confirmed") || conf.equals("high");
				String note = "[" + conf.toUpperCase() + "] " + ev;
				if (kind.equals("func")) {
					Function fn = getFunctionAt(a);
					if (fn == null) {
						disassemble(a);
						fn = createFunction(a, null);
					}
					if (fn == null) {
						say("cannot create function at " + a + " for " + name);
						continue;
					}
					if (strong) {
						fn.setName(name, SourceType.USER_DEFINED);
						fn.setComment(note);
						renamed++;
					}
					else {
						fn.setComment("proposed: " + name + " " + note);
						proposed++;
					}
				}
				else {
					if (strong) {
						createLabel(a, name, true, SourceType.USER_DEFINED);
						renamed++;
					}
					setPlateComment(a, (strong ? "" : "proposed: " + name + " ") + note);
					if (c.length > 5 && strong) {
						DataType dt = parseType(c[5]);
						try {
							clearListing(a, a.add(dt.getLength() - 1));
							createData(a, dt);
							data++;
						}
						catch (Exception e) {
							say("type not applied at " + a + ": " + e.getMessage());
						}
					}
				}
			}
		}
		say("symbols renamed/labelled: " + renamed + ", proposals as comments: " + proposed + ", data typed: " + data);
	}

	private void applySigs(File f) throws Exception {
		int n = 0;
		try (BufferedReader r = new BufferedReader(new FileReader(f))) {
			String line;
			while ((line = r.readLine()) != null) {
				if (line.isBlank() || line.startsWith("#")) {
					continue;
				}
				String[] c = line.split("\t");
				Function fn = getFunctionAt(toAddr(Long.parseLong(c[0], 16)));
				if (fn == null) {
					say("sig: no function at " + c[0]);
					continue;
				}
				List<Variable> params = new ArrayList<>();
				if (c.length > 2 && !c[2].isBlank()) {
					for (String p : c[2].split(",")) {
						String[] tn = p.trim().split("\\s+");
						params.add(new ParameterImpl(tn[1], parseType(tn[0]), currentProgram));
					}
				}
				fn.replaceParameters(params, FunctionUpdateType.DYNAMIC_STORAGE_ALL_PARAMS, true,
					SourceType.USER_DEFINED);
				fn.setReturnType(parseType(c[1]), SourceType.USER_DEFINED);
				n++;
			}
		}
		say("signatures applied: " + n);
	}
}
