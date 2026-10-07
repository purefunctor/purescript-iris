import * as $stylex from "@stylexjs/stylex";
export function nested(opacity) {
  return $stylex.keyframes({
    from: { opacity },
    to: { opacity: 1 }
  });
}
