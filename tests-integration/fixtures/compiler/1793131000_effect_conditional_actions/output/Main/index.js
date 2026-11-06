import * as Control_Applicative from "../Control.Applicative/index.js";
export function whenConfigured(configuration) {
  return (reference) => {
    const $condition = configuration.enabled;
    const $value = configuration.value;
    return () => {
      if ($condition) {
        reference.value = $value;
      }
      return;
    };
  };
}
export function unlessConfigured(configuration) {
  return (reference) => {
    const $condition = !configuration.enabled;
    const $value = configuration.value;
    return () => {
      if ($condition) {
        reference.value = $value;
      }
      return;
    };
  };
}
export function diagonal(size) {
  return () => {
    let count = 0 | 0;
    for (let row = 0 | 0; row < size; row++) {
      for (let column = 0 | 0; column < size; column++) {
        const $condition = row === column;
        if ($condition) {
          const section152 = count;
          const $value = section152 + (1 | 0) | 0;
          count = $value;
        }
      }
    }
    return count;
  };
}
export function genericWhen(applicativeEffectDict) {
  return /* @__PURE__ */ Control_Applicative.when(applicativeEffectDict);
}
