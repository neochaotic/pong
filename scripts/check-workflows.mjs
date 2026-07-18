#!/usr/bin/env node
/**
 * Parse every GitHub Actions workflow so a syntax error is caught here rather
 * than by a run that dies in 0s with "workflow file issue" and no usable log.
 *
 * The trap that motivated this: `with: { key: ${{ matrix.x }} }`. A `${{ }}`
 * expression inside a YAML *flow* mapping is read as nested flow syntax and
 * fails to parse — while looking perfectly reasonable.
 */
import { readdirSync, readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { dirname, join } from "node:path";

const dir = join(dirname(fileURLToPath(import.meta.url)), "..", ".github", "workflows");
let failed = false;

for (const file of readdirSync(dir).filter((f) => /\.ya?ml$/.test(f))) {
  const text = readFileSync(join(dir, file), "utf8");

  // Targeted check first: it gives a far clearer message than the parser error.
  const lines = text.split("\n");
  lines.forEach((line, i) => {
    if (/:\s*\{[^}]*\$\{\{/.test(line)) {
      console.error(
        `✗ ${file}:${i + 1} — \`\${{ }}\` inside a flow mapping { } will not parse.\n` +
          `    ${line.trim()}\n` +
          `    Rewrite it in block style (one key per line).`
      );
      failed = true;
    }
  });

  try {
    const { parse } = await import("yaml");
    const doc = parse(text);
    if (!doc?.jobs || Object.keys(doc.jobs).length === 0) {
      console.error(`✗ ${file}: no jobs defined`);
      failed = true;
    } else {
      console.log(`✓ ${file} — ${Object.keys(doc.jobs).join(", ")}`);
    }
  } catch (error) {
    console.error(`✗ ${file}: ${error.message}`);
    failed = true;
  }
}

if (failed) process.exit(1);
