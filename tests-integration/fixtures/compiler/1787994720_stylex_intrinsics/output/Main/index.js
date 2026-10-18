import * as $stylex from "@stylexjs/stylex";
export function buttonPropsArray(highlighted) {
  return $stylex.props([styles.button, highlighted && secondary.root]);
}
export function selectedProps(included) {
  return (alternate) => {
    return $stylex.props(included && (alternate ? styles.button : styles.label));
  };
}
export const animation = $stylex.keyframes({
  from: { opacity: 0 },
  to: { opacity: 1 }
});
export const styles = $stylex.create({
  button: {
    color: "red",
    padding: 8 | 0,
    marginInline: -20 | 0,
    opacity: -.5,
    animationName: animation,
    ":hover": { color: "blue" }
  },
  label: { fontWeight: 600 | 0 }
});
export const secondary = $stylex.create({ root: { backgroundColor: "navy" } });
export const buttonProps = $stylex.props(styles.button);
export const styleProps = {
  button: $stylex.props(styles.button),
  label: $stylex.props(styles.label)
};
export const appliedStyleProps = {
  button: $stylex.props(styles.button),
  label: $stylex.props(styles.label)
};
export const flippedStyleProps = {
  button: $stylex.props(styles.button),
  label: $stylex.props(styles.label)
};
export const buttonClassName = buttonProps.className;
export const labelClassName = styleProps.label.className;
