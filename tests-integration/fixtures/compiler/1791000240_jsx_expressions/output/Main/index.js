import * as Control_Category from "../Control.Category/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Ord from "../Data.Ord/index.js";
import * as Data_Semigroup from "../Data.Semigroup/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Widgets from "../Widgets/index.js";
import { jsx as $jsx, jsxs as $jsxs, Fragment as $Fragment } from "react/jsx-runtime";
import * as $foreign from "./foreign.js";
export function renderLabel(props) {
  return /* @__PURE__ */ Data_Show.show(Data_Show.showInt)(/* @__PURE__ */ props.value(Data_Show.showInt));
}
export function renderPoly(props) {
  return props.apply("polymorphic");
}
export function test5(render) {
  return $jsx(render, { value: 17 | 0 });
}
export function less(left) {
  return (right) => {
    return /* @__PURE__ */ Data_Ord.lessThan(Data_Ord.ordInt)(left)(right);
  };
}
export function keywordLabel(right) {
  return /* @__PURE__ */ Data_Ord.lessThan(Data_Ord.ordInt)({ then: 1 | 0 }.then)(right);
}
export const optional = $foreign["optional"];
const functorArrayDictMap = /* @__PURE__ */ Data_Functor.map(Data_Functor.functorArray);
export const badge = /* @__PURE__ */ (() => {
  const $closure = (props) => {
    return $jsx("strong", {
      title: props.label,
      children: props.children
    });
  };
  return $closure;
})();
export const label = /* @__PURE__ */ (() => {
  return renderLabel;
})();
export const poly = /* @__PURE__ */ (() => {
  return renderPoly;
})();
export const test = /* @__PURE__ */ (() => {
  const $field = 3 | 0;
  const $reactType = Widgets.button;
  const $element = $jsx(badge, {
    label: "local",
    children: $jsx($reactType, { label: "qualified" })
  });
  const $reactType$1 = Widgets.button;
  const $element$1 = $jsx($reactType$1, { label: "alias" });
  const $element$2 = $jsx("my-widget", { title: "a\"b" });
  const $element$3 = $jsxs($Fragment, { children: [
    "first",
    $jsx("span", {}),
    "last"
  ] });
  const $element$4 = $jsx(label, { value: (showIntDict) => 42 | 0 }, "answer");
  return $jsxs("main", {
    className: "greeting",
    "data-count": $field,
    hidden: false,
    children: [
      "Hello ",
      "世界",
      "!",
      $element,
      $element$1,
      $element$2,
      $element$3,
      $element$4,
      $jsx(poly, { apply: /* @__PURE__ */ Control_Category.identity(Control_Category.categoryFn) })
    ]
  });
})();
const test_ = /* @__PURE__ */ (() => {
  return $jsx("p", { children: "-- text, not a comment &amp; 🌱" });
})();
export const test2 = /* @__PURE__ */ (() => {
  return $jsxs($Fragment, { children: [$jsx("i", {})] });
})();
export const test3 = (() => {
  const $field = {
    close: "}",
    nested: { value: 7 | 0 }
  };
  const $element = $jsx("b", { children: "let" });
  let $result;
  $case: {
    if (true === true) {
      $result = $jsx("yes", {});
      break $case;
    }
    if (true === false) {
      $result = $jsx("no", {});
      break $case;
    }
    throw new Error("Pattern match failure");
  }
  return $jsxs("div", {
    info: $field,
    children: [
      $element,
      $result,
      $jsx("end", {})
    ]
  });
})();
export const test4 = /* @__PURE__ */ (() => {
  const $value = $jsx("div", { children: "outdented hole" });
  return () => {
    return $value;
  };
})();
export const test6 = /* @__PURE__ */ (() => {
  return $jsx("input", { transform: (section358) => section358 + (1 | 0) | 0 });
})();
export const test7 = /* @__PURE__ */ (() => {
  return $jsx("p", { children: "first second third" });
})();
export const test8 = /* @__PURE__ */ (() => {
  const $effect = () => {
    const $value = (5 | 0) + (2 | 0) | 0;
    return $value;
  };
  return $jsx("button", { action: $effect });
})();
export const ordinary = /* @__PURE__ */ (() => {
  return $jsx(label, { value: (showIntDict) => 5 | 0 });
})();
export const test9 = /* @__PURE__ */ (() => {
  return $jsx(label, { value: (showIntDict) => 5 | 0 });
})();
export const byTick = /* @__PURE__ */ (() => {
  return $jsx(label, { value: (showIntDict) => 11 | 0 });
})();
export const byQualified = /* @__PURE__ */ (() => {
  return $jsx(label, { value: (showIntDict) => -1 | 0 });
})();
export const open = /* @__PURE__ */ (() => {
  const $closure = (props) => {
    return $jsx("div", { children: props.label });
  };
  return $closure;
})();
export const test10 = /* @__PURE__ */ (() => {
  return $jsx(open, {
    label: "open",
    count: 42 | 0
  });
})();
export const test11 = /* @__PURE__ */ (() => {
  const $reactType = /* @__PURE__ */ optional({});
  return $jsx($reactType, { label: "subset" });
})();
export const test12 = /* @__PURE__ */ (() => {
  const $reactType = /* @__PURE__ */ optional({});
  return $jsx($reactType, { count: 7 | 0 });
})();
export const test13 = /* @__PURE__ */ (() => {
  return $jsx($Fragment, { children: /* @__PURE__ */ functorArrayDictMap((value) => value)(["first", "second"]) });
})();
export const test14 = /* @__PURE__ */ (() => {
  const $closure = (dictionary) => {
    return (target) => {
      return (props) => {
        return $jsx(target, props);
      };
    };
  };
  return /* @__PURE__ */ functorArrayDictMap(/* @__PURE__ */ $closure({})(Widgets.button))([{ label: "partial" }]);
})();
export const operators = {
  less: /* @__PURE__ */ Data_Ord.lessThan(Data_Ord.ordInt),
  append: /* @__PURE__ */ Data_Semigroup.append(Data_Semigroup.semigroupString),
  map: functorArrayDictMap
};
export const operatorResults = {
  less: operators.less(2 | 0)(7 | 0),
  append: operators.append("a")("b"),
  map: operators.map((section657) => section657 + (2 | 0) | 0)([3 | 0, 8 | 0])
};
export { test_ as "test'" };
