import * as Data_Unit from "../Data.Unit/index.js";
import * as $runtime from "../runtime.js";
export function call(k) {
  return k(Data_Unit.unit);
}
export const f = /* @__PURE__ */ (() => {
  let $lazy_go;
  $lazy_go = $runtime.binding("go", () => {
    return call(($unit) => $lazy_go());
  });
  const go = $lazy_go();
  return go;
})();
