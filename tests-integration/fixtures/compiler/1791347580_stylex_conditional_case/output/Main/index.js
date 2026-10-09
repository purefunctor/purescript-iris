import { breakpoints as Tokens_breakpoints } from "../Tokens/index.stylex.js";
import * as $stylex from "@stylexjs/stylex";
export const print = "@media print";
export const styles = $stylex.create({ root: { padding: {
  default: 8 | 0,
  [Tokens_breakpoints.small]: 4 | 0,
  [print]: 0 | 0,
  ["@media (min-width: 1200px)"]: 16 | 0,
  [$stylex.when.ancestor(":hover")]: 12 | 0
} } });
