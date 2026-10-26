import * as Data_Tuple from "../Data.Tuple/index.js";
export function letIn(t) {
  if (t.tag === "Tuple") {
    const { _1: x, _2: x$1 } = t;
    return x$1;
  } else {
    throw new Error("Pattern match failure");
  }
}
export function whereBound(t) {
  if (t.tag === "Tuple") {
    const { _1: x, _2: x$1 } = t;
    return x$1;
  } else {
    throw new Error("Pattern match failure");
  }
}
