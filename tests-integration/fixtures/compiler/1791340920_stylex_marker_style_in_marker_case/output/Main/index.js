import { rowMarker } from "./index.stylex.js";
import * as $stylex from "@stylexjs/stylex";
export const styles = $stylex.create({ row: { color: {
  default: "blue",
  [$stylex.when.ancestor(":hover", rowMarker)]: "red"
} } });
export { rowMarker } from "./index.stylex.js";
