import * as $stylex from "@stylexjs/stylex";
export function nested(colour) {
  return $stylex.create({ root: { color: colour } });
}
