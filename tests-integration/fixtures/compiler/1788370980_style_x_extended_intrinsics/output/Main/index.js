import { constants, variables, rowMarker } from "./index.stylex.js";
import * as $stylex from "@stylexjs/stylex";
export const theme = $stylex.createTheme(variables, {
  accent: $stylex.types.color("white"),
  length: $stylex.types.length("12px")
});
export const styles = $stylex.create({ root: {
  color: {
    default: "blue",
    [$stylex.when.ancestor(":hover")]: "red",
    [$stylex.when.ancestor(":focus", rowMarker)]: "green",
    [$stylex.when.descendant(":hover")]: "purple",
    [$stylex.when.descendant(":focus", rowMarker)]: "pink",
    [$stylex.when.siblingBefore(":hover")]: "orange",
    [$stylex.when.siblingBefore(":focus", rowMarker)]: "yellow",
    [$stylex.when.siblingAfter(":hover")]: "gray",
    [$stylex.when.siblingAfter(":focus", rowMarker)]: "black",
    [$stylex.when.anySibling(":hover")]: "navy",
    [$stylex.when.anySibling(":focus", rowMarker)]: "teal"
  },
  position: $stylex.firstThatWorks("sticky", "fixed")
} });
export const attributes = $stylex.attrs(styles.root);
export const recordAttributes = { root: $stylex.attrs(styles.root) };
export const markerProps = $stylex.props(rowMarker);
export const defaultMarkerProps = $stylex.props($stylex.defaultMarker());
export const transitionClass = $stylex.viewTransitionClass({
  new: { opacity: 1 },
  old: { opacity: 0 }
});
export const fallback = $stylex.positionTry({
  positionArea: "block-start",
  margin: 8 | 0
});
export { constants, variables, rowMarker } from "./index.stylex.js";
