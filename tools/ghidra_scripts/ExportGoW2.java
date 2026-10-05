// Exports decompiler output, function index, call graph and control-flow graphs.
// Usage (headless): -postScript ExportGoW2.java <outDir>
//@category GoW2
import java.io.*;
import java.util.*;

import ghidra.app.decompiler.*;
import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.*;
import ghidra.program.model.block.*;
import ghidra.program.model.data.*;
import ghidra.program.model.listing.*;
import ghidra.program.model.symbol.*;
import ghidra.util.task.TaskMonitor;

public class ExportGoW2 extends GhidraScript {

	private static final int FUNCS_PER_FILE = 200;

	@Override
	protected void run() throws Exception {
		String[] args = getScriptArgs();
		File out = new File(args.length > 0 ? args[0] : "analysis");
		File decompDir = new File(out, "decomp");
		decompDir.mkdirs();

		Listing listing = currentProgram.getListing();
		FunctionManager fm = currentProgram.getFunctionManager();
		ReferenceManager rm = currentProgram.getReferenceManager();
		List<Function> funcs = new ArrayList<>();
		for (Function f : fm.getFunctions(true)) {
			if (!f.isExternal() && !f.isThunk()) {
				funcs.add(f);
			}
		}
		println("functions: " + funcs.size());

		DecompInterface di = new DecompInterface();
		DecompileOptions opts = new DecompileOptions();
		di.setOptions(opts);
		di.toggleCCode(true);
		di.toggleSyntaxTree(false);
		di.setSimplificationStyle("decompile");
		di.openProgram(currentProgram);

		BasicBlockModel bbm = new BasicBlockModel(currentProgram);

		try (PrintWriter idx = new PrintWriter(new FileWriter(new File(out, "functions.tsv")));
				PrintWriter cg = new PrintWriter(new FileWriter(new File(out, "callgraph.tsv")));
				PrintWriter cfg = new PrintWriter(new FileWriter(new File(out, "cfg.jsonl")))) {
			idx.println("addr\tname\tsize\tblocks\tcallers\tcallees\tdecomp_file\tstrings");
			cg.println("caller\tcallee");

			PrintWriter dec = null;
			String decName = null;
			int n = 0;
			for (Function f : funcs) {
				monitor.checkCancelled();
				if (n % FUNCS_PER_FILE == 0) {
					if (dec != null) {
						dec.close();
					}
					decName = String.format("%04d_%s.c", n / FUNCS_PER_FILE, f.getEntryPoint());
					dec = new PrintWriter(new FileWriter(new File(decompDir, decName)));
					dec.println("// God of War II (SCUS-97481) - Ghidra decompiler output, unedited.");
					dec.println("#include \"gow2_types.h\"\n");
				}
				n++;

				// --- control flow graph
				List<String> blocks = new ArrayList<>();
				List<String> edges = new ArrayList<>();
				CodeBlockIterator it = bbm.getCodeBlocksContaining(f.getBody(), monitor);
				while (it.hasNext()) {
					CodeBlock b = it.next();
					blocks.add(String.format("[\"%s\",\"%s\"]", b.getMinAddress(), b.getMaxAddress()));
					CodeBlockReferenceIterator dst = b.getDestinations(monitor);
					while (dst.hasNext()) {
						CodeBlockReference r = dst.next();
						if (r.getFlowType().isCall()) {
							continue;
						}
						Address to = r.getDestinationAddress();
						if (f.getBody().contains(to)) {
							edges.add(String.format("[\"%s\",\"%s\",\"%s\"]", b.getMinAddress(), to,
								r.getFlowType().getName()));
						}
					}
				}
				cfg.printf("{\"addr\":\"%s\",\"name\":\"%s\",\"blocks\":[%s],\"edges\":[%s]}%n", f.getEntryPoint(),
					f.getName(), String.join(",", blocks), String.join(",", edges));

				// --- call graph
				Set<Function> callees = f.getCalledFunctions(monitor);
				for (Function c : callees) {
					cg.printf("%s\t%s%n", f.getEntryPoint(), c.getEntryPoint());
				}
				int callers = f.getCallingFunctions(monitor).size();

				// --- referenced string literals (key evidence for naming)
				LinkedHashSet<String> strs = new LinkedHashSet<>();
				for (Address a : f.getBody().getAddresses(true)) {
					for (Reference r : rm.getReferencesFrom(a)) {
						Data d = listing.getDataAt(r.getToAddress());
						if (d != null && d.hasStringValue()) {
							String s = d.getDefaultValueRepresentation();
							strs.add(s.replace('\t', ' ').replace('\n', ' '));
						}
					}
				}

				// --- decompile
				dec.printf("// ---- %s @ %s  size=%d  callers=%d%n", f.getName(), f.getEntryPoint(),
					f.getBody().getNumAddresses(), callers);
				for (String s : strs) {
					dec.println("//   str: " + s);
				}
				DecompileResults res = di.decompileFunction(f, 60, monitor);
				if (res != null && res.decompileCompleted()) {
					dec.println(res.getDecompiledFunction().getC());
				}
				else {
					dec.println("// DECOMPILATION FAILED: " + (res == null ? "null" : res.getErrorMessage()));
				}

				idx.printf("%s\t%s\t%d\t%d\t%d\t%d\t%s\t%s%n", f.getEntryPoint(), f.getName(),
					f.getBody().getNumAddresses(), blocks.size(), callers, callees.size(), decName,
					String.join(" | ", strs));
				if (n % 500 == 0) {
					println("decompiled " + n + "/" + funcs.size());
				}
			}
			if (dec != null) {
				dec.close();
			}
		}
		finally {
			di.dispose();
		}
		println("export done -> " + out.getAbsolutePath());
	}
}
