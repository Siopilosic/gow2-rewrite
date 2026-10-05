// Pre-analysis: enable analyzers that are off by default but matter for PS2 code.
//@category GoW2
import java.util.Map;

import ghidra.app.script.GhidraScript;

public class EnableParamId extends GhidraScript {
	@Override
	protected void run() throws Exception {
		Map<String, String> opts = getCurrentAnalysisOptionsAndValues(currentProgram);
		for (String name : new String[] { "Decompiler Parameter ID", "Aggressive Instruction Finder" }) {
			if (opts.containsKey(name)) {
				setAnalysisOption(currentProgram, name, "true");
				println("enabled analyzer: " + name);
			}
			else {
				println("analyzer not found: " + name);
			}
		}
	}
}
