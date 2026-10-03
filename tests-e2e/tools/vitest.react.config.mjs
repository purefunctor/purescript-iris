import path from "node:path";
import { fileURLToPath } from "node:url";

import { playwright } from "@vitest/browser-playwright";
import { defineConfig } from "vitest/config";

const tools = path.dirname(fileURLToPath(import.meta.url));

export default defineConfig({
  define: {
    "process.env.IRIS_REACT_OUTPUT": JSON.stringify(process.env.IRIS_REACT_OUTPUT),
  },
  resolve: {
    alias: {
      react: path.join(tools, "node_modules/react"),
      "react-dom": path.join(tools, "node_modules/react-dom"),
    },
    dedupe: ["react", "react-dom"],
  },
  optimizeDeps: {
    include: ["react", "react/jsx-runtime", "react-dom/client"],
  },
  server: {
    fs: { allow: [tools, process.env.IRIS_REACT_OUTPUT] },
  },
  test: {
    include: [path.join(tools, "verify-react.browser.js")],
    browser: {
      enabled: true,
      headless: true,
      provider: playwright(),
      instances: [{ browser: "chromium" }],
    },
  },
});
