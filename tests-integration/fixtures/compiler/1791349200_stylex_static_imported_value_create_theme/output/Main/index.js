import { accent as Tokens_accent, variables as Tokens_variables } from "../Tokens/index.js";
import * as $stylex from "@stylexjs/stylex";
export const theme = $stylex.createTheme(Tokens_variables, { accent: Tokens_accent });
