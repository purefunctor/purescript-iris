import * as $stylex from "@stylexjs/stylex";
export const fonts = ["Inter", "sans-serif"];
export const styles = $stylex.create({ root: (font) => ({ fontFamily: [...fonts, font] }) });
