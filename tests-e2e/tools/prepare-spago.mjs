import { spawnSync } from "node:child_process";
import fs from "node:fs";
import os from "node:os";
import path from "node:path";
import process from "node:process";
import { fileURLToPath } from "node:url";

const toolsDirectory = path.dirname(fileURLToPath(import.meta.url));
const spago = path.join(toolsDirectory, "node_modules/spago/bin/bundle.js");
const pursShims = path.join(toolsDirectory, "../../compiler-executable/iris-spago/bundled/purs");

const temporary = fs.mkdtempSync(path.join(os.tmpdir(), "iris-e2e-spago-"));
try {
  const shimDirectory = path.join(temporary, "bin");
  fs.mkdirSync(shimDirectory);
  fs.copyFileSync(path.join(pursShims, "purs.sh"), path.join(shimDirectory, "purs"));
  fs.chmodSync(path.join(shimDirectory, "purs"), 0o755);
  fs.copyFileSync(path.join(pursShims, "purs.cmd"), path.join(shimDirectory, "purs.cmd"));

  const workspace = path.join(temporary, "workspace");
  fs.mkdirSync(workspace);
  fs.writeFileSync(
    path.join(workspace, "spago.yaml"),
    "package:\n  name: application\n  dependencies: []\nworkspace: {}\n",
  );

  const pathVariable =
    Object.keys(process.env).find((key) => key.toUpperCase() === "PATH") ?? "PATH";
  const searchPath = [shimDirectory, process.env[pathVariable]].filter(Boolean);
  const environment = { ...process.env, [pathVariable]: searchPath.join(path.delimiter) };
  const result = spawnSync(process.execPath, [spago, "fetch"], {
    cwd: workspace,
    env: environment,
    stdio: "inherit",
  });
  if (result.error) {
    throw result.error;
  }
  if (result.status !== 0) {
    process.exitCode = result.status ?? 1;
  }
} finally {
  fs.rmSync(temporary, { recursive: true, force: true });
}
