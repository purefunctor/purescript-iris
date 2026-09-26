export const Parser = ($value0) => ({
  tag: "Parser",
  _1: $value0
});
export function bindFlipped(callback) {
  return ($parser) => {
    if ($parser.tag === "Parser") {
      const { _1: parser } = $parser;
      const $closure = (state) => {
        const $scrutinee = callback(parser(state));
        if ($scrutinee.tag === "Parser") {
          const { _1: next } = $scrutinee;
          return next(state);
        }
        throw new Error("Pattern match failure");
      };
      return {
        tag: "Parser",
        _1: $closure
      };
    } else {
      throw new Error("Pattern match failure");
    }
  };
}
export function fail(message) {
  return bindFlipped(($int) => ({
    tag: "Parser",
    _1: ($int$1) => message
  }))(position);
}
export function choose(condition) {
  if (condition) {
    return conditional;
  } else {
    return 19 | 0;
  }
}
export const position = {
  tag: "Parser",
  _1: (state) => state
};
export const plusParser = { empty: fail(7 | 0) };
export const test = (() => {
  const $scrutinee = plusParser.empty;
  if ($scrutinee.tag === "Parser") {
    const { _1: parser } = $scrutinee;
    return parser(42 | 0);
  }
  throw new Error("Pattern match failure");
})();
export const conditional = choose(false);
