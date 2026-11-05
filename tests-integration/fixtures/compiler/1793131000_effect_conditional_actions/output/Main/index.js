import * as Control_Applicative from "../Control.Applicative/index.js";
import * as Effect from "../Effect/index.js";
export function whenConfigured(configuration) {
  return (reference) => {
    const $function = Control_Applicative.when(Effect.applicativeEffect)(configuration.enabled);
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    return /* @__PURE__ */ $function($effect);
  };
}
export function unlessConfigured(configuration) {
  return (reference) => {
    const $function = Control_Applicative.unless(Effect.applicativeEffect)(configuration.enabled);
    const $value = configuration.value;
    const $effect = () => {
      reference.value = $value;
      return undefined;
    };
    return /* @__PURE__ */ $function($effect);
  };
}
export function diagonal(size) {
  return () => {
    let count = 0 | 0;
    for (let row = 0 | 0; row < size; row++) {
      for (let column = 0 | 0; column < size; column++) {
        const $function = Control_Applicative.when(Effect.applicativeEffect)(row === column);
        const $effect = () => {
          const section152 = count;
          const $value = section152 + (1 | 0) | 0;
          count = $value;
          return undefined;
        };
        (/* @__PURE__ */ $function($effect))();
      }
    }
    return count;
  };
}
export function genericWhen(applicativeEffectDict) {
  return /* @__PURE__ */ Control_Applicative.when(applicativeEffectDict);
}
