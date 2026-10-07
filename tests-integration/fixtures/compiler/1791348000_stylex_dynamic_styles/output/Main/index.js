import * as $stylex from "@stylexjs/stylex";
export const styles = $stylex.create({
  base: { color: "red" },
  sized: (size) => ({
    width: size.width,
    height: size.height
  }),
  faded: (opacity) => ({ opacity: {
    default: opacity,
    [$stylex.when.ancestor(":hover")]: 1
  } })
});
export const sizedProps = $stylex.props(styles.sized({
  width: 100 | 0,
  height: "50%"
}));
export const fadedProps = $stylex.props([styles.base, styles.faded(.5)]);
export const fadedAttrs = $stylex.attrs(styles.faded(.25));
export const baseProps = $stylex.props(styles.base);
