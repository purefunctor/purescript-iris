import { accent as Tokens_accent } from "../Tokens/index.js";
import * as $stylex from "@stylexjs/stylex";
export const animation = $stylex.keyframes({
  from: { color: Tokens_accent },
  to: { color: "blue" }
});
