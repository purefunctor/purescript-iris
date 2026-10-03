import path from "node:path";
import process from "node:process";
import { spawnSync } from "node:child_process";
import { fileURLToPath } from "node:url";

const tools = path.dirname(fileURLToPath(import.meta.url));
const output = path.resolve(process.argv[2]);
const vitest = path.join(tools, "node_modules/vitest/vitest.mjs");
const config = path.join(tools, "vitest.react.config.mjs");
const result = spawnSync(process.execPath, [vitest, "run", "--config", config], {
  cwd: tools,
  env: { ...process.env, IRIS_REACT_OUTPUT: output.replaceAll("\\", "/") },
  stdio: "inherit",
});

if (result.error) throw result.error;
process.exitCode = result.status ?? 1;
