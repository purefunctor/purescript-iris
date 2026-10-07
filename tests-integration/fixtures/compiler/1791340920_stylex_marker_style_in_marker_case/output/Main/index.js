import * as $stylex from "@stylexjs/stylex";
export const rowMarker = $stylex.defineMarker();
export const styles = $stylex.create({ row: { color: {
  default: "blue",
  [$stylex.when.ancestor(":hover", rowMarker)]: "red"
} } });
