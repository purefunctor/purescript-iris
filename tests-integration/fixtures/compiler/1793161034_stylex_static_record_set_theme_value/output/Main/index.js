import { card } from "./index.stylex.js";
import * as $stylex from "@stylexjs/stylex";
export const styles = /* @__PURE__ */ (() => {
  return $stylex.create({ root: {
    ...card,
    color: "teal"
  } });
})();
export { card } from "./index.stylex.js";
