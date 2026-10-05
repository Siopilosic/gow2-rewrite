// Applies names from analysis/symbols.tsv (addr, kind, name, confidence, evidence).
// Usage: -postScript ApplySymbols.java <symbols.tsv>
//@category GoW2
import java.io.*;

import ghidra.app.script.GhidraScript;
import ghidra.program.model.address.Address;
import ghidra.program.model.listing.Function;
import ghidra.program.model.symbol.SourceType;

public class ApplySymbols extends GhidraScript {
	@Override
	protected void run() throws Exception {
		String[] args = getScriptArgs();
		File f = new File(args.length > 0 ? args[0] : askFile("symbols.tsv", "Apply").getPath());
		int ok = 0, bad = 0;
		try (BufferedReader r = new BufferedReader(new FileReader(f))) {
			String line;
			while ((line = r.readLine()) != null) {
				if (line.isBlank() || line.startsWith("#")) {
					continue;
				}
				String[] c = line.split("\t");
				Address a = toAddr(Long.parseLong(c[0], 16));
				String kind = c[1], name = c[2];
				String note = c.length > 4 ? "[" + c[3] + "] " + c[4] : null;
				if (kind.equals("func")) {
					Function fn = getFunctionAt(a);
					if (fn == null) {
						fn = createFunction(a, name);
					}
					if (fn == null) {
						println("no function at " + a + " for " + name);
						bad++;
						continue;
					}
					fn.setName(name, SourceType.USER_DEFINED);
					if (note != null) {
						fn.setComment(note);
					}
				}
				else {
					createLabel(a, name, true, SourceType.USER_DEFINED);
					if (note != null) {
						setPlateComment(a, note);
					}
				}
				ok++;
			}
		}
		println("applied " + ok + " symbols, " + bad + " failed");
	}
}
