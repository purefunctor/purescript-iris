export function predicate(dictionary) {
  return dictionary.predicate;
}
export function defaultValue(dictionary) {
  return dictionary.defaultValue;
}
export function checked(condition) {
  if (condition) {
    return 1 | 0;
  } else {
    return 2 | 0;
  }
}
export function inferred(condition) {
  if (condition) {
    return "yes";
  } else {
    return "no";
  }
}
export function constrained(predicateValueDict) {
  return (value) => /* @__PURE__ */ predicate(predicateValueDict)(value) ? 1 | 0 : 2 | 0;
}
export function constrainedBranches(defaultValueDict) {
  return (condition) => condition ? /* @__PURE__ */ defaultValue(defaultValueDict) : /* @__PURE__ */ defaultValue(defaultValueDict);
}
export function higherRank(condition) {
  if (condition) {
    return identity;
  } else {
    return identity;
  }
}
export function nested(first) {
  return (second) => {
    if (first) {
      if (second) {
        return 1 | 0;
      } else {
        return 2 | 0;
      }
    } else {
      return 3 | 0;
    }
  };
}
export function asArgument(condition) {
  return identity(condition ? 1 | 0 : 2 | 0);
}
export function identity(value) {
  return value;
}
