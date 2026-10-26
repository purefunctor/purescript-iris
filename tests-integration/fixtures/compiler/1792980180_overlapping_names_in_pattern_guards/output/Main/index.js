import * as Data_Boolean from "../Data.Boolean/index.js";
import * as Data_Tuple from "../Data.Tuple/index.js";
export function withinGuard(t) {
  if (t.tag === "Tuple") {
    const { _1: x, _2: x$1 } = t;
    return x$1;
  }
  if (Data_Boolean.otherwise) {
    return 0 | 0;
  }
  throw new Error("Pattern match failure");
}
export function shadowsArgument(x) {
  return (t) => {
    if (t.tag === "Tuple") {
      const { _1: x$1 } = t;
      return x$1;
    }
    if (Data_Boolean.otherwise) {
      return 0 | 0;
    }
    throw new Error("Pattern match failure");
  };
}
