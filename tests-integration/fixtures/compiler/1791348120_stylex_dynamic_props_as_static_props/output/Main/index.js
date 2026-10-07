import * as $stylex from "@stylexjs/stylex";
export const styles = $stylex.create({ sized: (width) => ({ width }) });
export const sizedProps = $stylex.props(styles.sized(4 | 0));
