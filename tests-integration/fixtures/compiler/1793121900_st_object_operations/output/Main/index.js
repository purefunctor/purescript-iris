import * as Foreign_Object from "../Foreign.Object/index.js";
import * as Foreign_Object_ST from "../Foreign.Object.ST/index.js";
export function built(key) {
  const $function = Foreign_Object.runST;
  const $action = Foreign_Object_ST.new;
  const $effect = () => {
    const object = $action();
    const $action$1 = Foreign_Object_ST.poke("a")(1 | 0)(object);
    const $sTObject = $action$1();
    const $action$2 = Foreign_Object_ST.poke(key)(2 | 0)(object);
    const poked = $action$2();
    const $action$3 = Foreign_Object_ST.poke("removed")(3 | 0)(poked);
    const $sTObject$1 = $action$3();
    return Foreign_Object_ST.delete("removed")(poked)();
  };
  return $function($effect);
}
export function copies(original) {
  const $action = Foreign_Object.thawST(original);
  const object = $action();
  const $action$1 = Foreign_Object_ST.poke("b")(2 | 0)(object);
  const $sTObject = $action$1();
  const $action$2 = Foreign_Object.freezeST(object);
  const frozen = $action$2();
  const $action$3 = Foreign_Object_ST.delete("a")(object);
  const $sTObject$1 = $action$3();
  const $action$4 = Foreign_Object.freezeST(object);
  const final = $action$4();
  const $value = {
    original,
    frozen,
    final
  };
  return $value;
}
