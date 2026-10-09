import * as $stylex from "@stylexjs/stylex";
export const constants = $stylex.defineConsts({
  compact: "@media (max-width: 40rem)",
  columns: 12 | 0
});
export const variables = $stylex.defineVars({
  accent: $stylex.types.color("royalblue"),
  angle: $stylex.types.angle("45deg"),
  image: $stylex.types.image("linear-gradient(red, blue)"),
  integer: $stylex.types.integer(1 | 0),
  length: $stylex.types.length("8px"),
  lengthPercentage: $stylex.types.lengthPercentage("10%"),
  number: $stylex.types.number(.5),
  percentage: $stylex.types.percentage("50%"),
  resolution: $stylex.types.resolution("2dppx"),
  time: $stylex.types.time("200ms"),
  transformFunction: $stylex.types.transformFunction("scale(1)"),
  transformList: $stylex.types.transformList("scale(1) rotate(2deg)"),
  url: $stylex.types.url("url(image.png)")
});
export const rowMarker = $stylex.defineMarker();
