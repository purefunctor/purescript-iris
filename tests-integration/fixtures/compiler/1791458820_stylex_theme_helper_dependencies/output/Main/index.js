import { spacing, variables } from "./index.stylex.js";
import * as $stylex from "@stylexjs/stylex";
export const gap = "21px";
const sizes = { gap };
const gapStyles = $stylex.create({ root: {
  margin: gap,
  color: variables.accent
} });
export const gapProps = $stylex.props(gapStyles.root);
export { spacing, variables } from "./index.stylex.js";
