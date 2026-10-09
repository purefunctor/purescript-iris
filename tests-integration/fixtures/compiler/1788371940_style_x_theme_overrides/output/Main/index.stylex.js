import * as $stylex from "@stylexjs/stylex";
export const variables = $stylex.defineVars({
  accent: $stylex.types.color("blue"),
  spacing: $stylex.types.length("8px")
});
export const invalidInteger = $stylex.defineVars({ value: $stylex.types.integer(true) });
export const invalidNumber = $stylex.defineVars({ value: $stylex.types.number("1") });
export const invalidColor = $stylex.defineVars({ value: $stylex.types.color(42 | 0) });
