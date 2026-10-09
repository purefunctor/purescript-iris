import { variables, invalidInteger, invalidNumber, invalidColor } from "./index.stylex.js";
import * as $stylex from "@stylexjs/stylex";
export const valid = $stylex.createTheme(variables, { accent: $stylex.types.color("white") });
export const wrongType = $stylex.createTheme(variables, { accent: $stylex.types.length("12px") });
export const unknown = $stylex.createTheme(variables, { missing: $stylex.types.color("red") });
export { variables, invalidInteger, invalidNumber, invalidColor } from "./index.stylex.js";
