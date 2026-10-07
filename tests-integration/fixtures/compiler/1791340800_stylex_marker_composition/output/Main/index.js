import * as $stylex from "@stylexjs/stylex";
export const rowMarker = $stylex.defineMarker();
export const styles = $stylex.create({ row: { color: "red" } });
export const rowProps = $stylex.props([styles.row, rowMarker]);
export const rowAttrs = $stylex.attrs([rowMarker, styles.row]);
