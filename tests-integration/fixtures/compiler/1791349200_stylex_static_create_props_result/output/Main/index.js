import * as $stylex from "@stylexjs/stylex";
export const base = $stylex.create({ root: { color: "red" } });
export const properties = $stylex.props(base.root);
export const styles = $stylex.create({ root: { color: properties.className } });
