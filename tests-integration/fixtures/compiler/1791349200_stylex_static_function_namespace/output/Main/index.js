import * as $stylex from "@stylexjs/stylex";
export function styles(compileStyleListConsFunctionNilDict) {
  return $stylex.create({ root: (colour) => ({ color: colour }) });
}
