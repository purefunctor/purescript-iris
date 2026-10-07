import * as $stylex from "@stylexjs/stylex";
export function nested($string) {
  return $stylex.create({ root: { color: "red" } });
}
