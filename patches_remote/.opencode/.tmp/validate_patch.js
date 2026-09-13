#!/usr/bin/env node
/**
 * DROID patch validation harness using droid-lsp modules.
 * Usage: node validate_patch.js <patch.ini> [--json] [--ram]
 * Reports: parse errors, unknown circuits, unknown params, duplicate params,
 *          invalid jacks, undefined cables, duplicate cable defs, RAM budget.
 */
const path = require("path");
const fs = require("fs");
const DROID_LSP = process.env.DROID_LSP || "/home/bjoern/git/droid/droid-lsp";
// Resolve vscode-languageserver-textdocument from droid-lsp's own node_modules
const { TextDocument } = require(
	require.resolve("vscode-languageserver-textdocument", {
		paths: [path.join(DROID_LSP, "node_modules")],
	}),
);
const { parseDroidDocument } = require(path.join(DROID_LSP, "out/parser.js"));
const { loadSchema } = require(path.join(DROID_LSP, "out/schema.js"));
const { provideDiagnostics } = require(
	path.join(DROID_LSP, "out/diagnostics.js"),
);

const file = process.argv[2];
const asJson = process.argv.includes("--json");

if (!file) {
	console.error("Usage: node validate_patch.js <patch.ini> [--json]");
	process.exit(2);
}

const schema = loadSchema();
const text = fs.readFileSync(file, "utf8");
const doc = parseDroidDocument(text, schema);
const textDoc = TextDocument.create(`file://${file}`, "droid", 0, text);
const diags = provideDiagnostics(schema, doc, textDoc);

// RAM computation (mirrors diagnostics but always prints)
let ramUsed = 0;
let unknown = false;
const perSection = {};
for (const section of doc.sections) {
	const circuit = schema.circuits.get(section.name);
	const controller = schema.controllers.get(section.name);
	if (circuit) {
		ramUsed += circuit.ramsize;
		perSection[section.name] =
			(perSection[section.name] || 0) + circuit.ramsize;
	} else if (controller) {
		ramUsed += controller.ramsize;
		perSection[section.name] =
			(perSection[section.name] || 0) + controller.ramsize;
	} else {
		unknown = true;
	}
}

if (asJson) {
	const summary = {
		errors: 0,
		warnings: 0,
		hints: 0,
		ram: { used: ramUsed, unknown },
		diag: [],
	};
	for (const d of diags) {
		const sev =
			d.severity === 1 ? "error" : d.severity === 2 ? "warning" : "hint";
		if (sev === "error") summary.errors++;
		if (sev === "warning") summary.warnings++;
		if (sev === "hint") summary.hints++;
		summary.diag.push({ sev, line: d.range.start.line + 1, msg: d.message });
	}
	// Top RAM consumers
	summary.ram.top = Object.entries(perSection)
		.sort((a, b) => b[1] - a[1])
		.slice(0, 12)
		.map(([c, r]) => ({
			circuit: c,
			count: r / schema.circuits.get(c)?.ramsize,
			ram: r,
		}));
	console.log(JSON.stringify(summary, null, 2));
} else {
	console.log(`=== ${file} ===`);
	console.log(`Sections: ${doc.sections.length}`);
	console.log(
		`Cable defs: ${doc.cableDefs.length}, cable uses: ${doc.cableUses.length}`,
	);
	console.log(
		`RAM used: ${ramUsed} bytes${unknown ? " (some circuits unknown!)" : ""}`,
	);
	console.log(
		`Master RAM available: ${JSON.stringify(schema.availableMemory)}`,
	);
	const counts = { error: 0, warning: 0, hint: 0 };
	for (const d of diags) {
		const sev =
			d.severity === 1 ? "error" : d.severity === 2 ? "warning" : "hint";
		counts[sev]++;
	}
	console.log(
		`Diagnostics: ${counts.error} errors, ${counts.warning} warnings, ${counts.hint} hints`,
	);
	const interesting = diags.filter((d) => d.severity <= 2);
	if (interesting.length) {
		console.log("\n--- errors & warnings ---");
		for (const d of interesting) {
			const sev = d.severity === 1 ? "ERR " : "WARN";
			console.log(`${sev} L${d.range.start.line + 1}: ${d.message}`);
		}
	}
	// RAM top consumers
	console.log("\n--- top RAM consumers ---");
	for (const [c, r] of Object.entries(perSection)
		.sort((a, b) => b[1] - a[1])
		.slice(0, 12)) {
		const def = schema.circuits.get(c);
		const count = def ? r / def.ramsize : "?";
		console.log(`  ${c}: ${r} bytes (${count} × ${def ? def.ramsize : "?"})`);
	}
	const avail = Object.values(schema.availableMemory)[0];
	if (avail) {
		console.log(
			`\nRAM: ${ramUsed}/${avail} bytes used (${Math.round((ramUsed / avail) * 100)}%)`,
		);
	}
}
