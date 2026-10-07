import * as Tokens from "../Tokens/index.js";
import * as $stylex from "@stylexjs/stylex";
export const colour = Tokens.ordinary.accent;
export const styles = $stylex.create({ root: { color: colour } });
