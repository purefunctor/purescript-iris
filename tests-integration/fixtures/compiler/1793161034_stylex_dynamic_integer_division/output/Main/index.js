import * as $stylex from "@stylexjs/stylex";
import * as $runtime from "../runtime.js";
export const styles = $stylex.create({ root: (width) => ({
  width: /* @__PURE__ */ $runtime.intDiv(width, 2 | 0),
  height: /* @__PURE__ */ $runtime.intMod(width, 3 | 0)
}) });
