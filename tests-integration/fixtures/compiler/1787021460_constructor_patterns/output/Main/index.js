export const Empty = "Empty";
export const One = ($value0) => ({
  tag: "One",
  _1: $value0
});
export const Pair = ($value0) => ($value1) => ({
  tag: "Pair",
  _1: $value0,
  _2: $value1
});
export const Outer = ($value0) => ({
  tag: "Outer",
  _1: $value0
});
export function first($choice) {
  if ($choice === "Empty") {
    return "Empty";
  }
  if ($choice.tag === "One") {
    const { _1: value } = $choice;
    return {
      tag: "One",
      _1: value
    };
  }
  if ($choice.tag === "Pair") {
    const whole = $choice;
    const { _1: left } = $choice;
    if (whole.tag === "Pair") {
      return {
        tag: "One",
        _1: left
      };
    }
    return "Empty";
  }
  throw new Error("Pattern match failure");
}
export function pair($choice) {
  if ($choice.tag === "Pair") {
    const { _1: left, _2: right } = $choice;
    return {
      tag: "Pair",
      _1: left,
      _2: right
    };
  }
  const choice = $choice;
  return choice;
}
export function unwrap(value) {
  return value;
}
export function nested($nested) {
  if ($nested.tag === "Outer" && $nested._1.tag === "One") {
    const { _1: value } = $nested._1;
    return {
      tag: "One",
      _1: value
    };
  }
  return "Empty";
}
export function bind(value) {
  return (continuation) => {
    return continuation(value);
  };
}
export function ordinaryBind(identity) {
  return bind(identity)((value) => value);
}
export function partialBind(partialDict) {
  return (choice) => {
    return bind(choice)(($choice) => {
      if ($choice.tag === "One") {
        const { _1: value } = $choice;
        return value;
      } else {
        throw new Error("Pattern match failure");
      }
    });
  };
}
