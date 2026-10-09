import { "await" as Tokens_await } from "../Tokens/index.js";
import { rowMarker as Tokens_rowMarker, spacing as Tokens_spacing, variables as Tokens_variables } from "../Tokens/index.stylex.js";
import * as $stylex from "@stylexjs/stylex";
export const theme = $stylex.createTheme(Tokens_variables, { accent: "white" });
export const styles = $stylex.create({ root: {
  color: {
    default: "blue",
    [$stylex.when.ancestor(":hover", Tokens_rowMarker)]: "red"
  },
  padding: Tokens_spacing.gap
} });
export const awaitProps = $stylex.props(Tokens_await);
