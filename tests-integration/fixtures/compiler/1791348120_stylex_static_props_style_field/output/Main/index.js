import * as $stylex from "@stylexjs/stylex";
export const styles = $stylex.create({ base: { color: "red" } });
export const baseStyle = $stylex.props(styles.base).style;
