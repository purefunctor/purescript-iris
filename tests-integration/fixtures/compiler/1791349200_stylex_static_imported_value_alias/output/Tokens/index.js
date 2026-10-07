import * as $stylex from "@stylexjs/stylex";
export const accent = "red";
export const ordinary = { accent: "blue" };
export const variables = $stylex.defineVars({ accent: "navy" });
export const alias = variables;
