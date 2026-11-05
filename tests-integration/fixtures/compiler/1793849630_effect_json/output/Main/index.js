import * as Control_Bind from "../Control.Bind/index.js";
import * as Data_Either from "../Data.Either/index.js";
import * as Data_Function from "../Data.Function/index.js";
import * as Data_Functor from "../Data.Functor/index.js";
import * as Data_Maybe from "../Data.Maybe/index.js";
import * as Data_Show from "../Data.Show/index.js";
import * as Iris_Effect_Json from "../Iris.Effect.Json/index.js";
function parsedInt(text) {
  return Data_Either.either(Data_Function.const("Nothing"))(Iris_Effect_Json.toInt)(Iris_Effect_Json.parse(text));
}
function describeParse(text) {
  const $scrutinee = Iris_Effect_Json.parse(text);
  if ($scrutinee.tag === "Left") {
    return "invalid";
  }
  if ($scrutinee.tag === "Right") {
    const { _1: json } = $scrutinee;
    const $scrutinee$1 = /* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("product")(json))(Iris_Effect_Json.toString);
    if ($scrutinee$1.tag === "Just") {
      const { _1: product } = $scrutinee$1;
      return "product " + product;
    }
    if ($scrutinee$1 === "Nothing") {
      return "no product";
    }
    throw new Error("Pattern match failure");
  }
  throw new Error("Pattern match failure");
}
const bindMaybeDictBind = /* @__PURE__ */ Control_Bind.bind(Data_Maybe.bindMaybe);
const showMaybeIntDict = /* @__PURE__ */ Data_Maybe.showMaybe(Data_Show.showInt);
const functorArrayDictMap = /* @__PURE__ */ Data_Functor.map(Data_Functor.functorArray);
const showMaybeStringDict = /* @__PURE__ */ Data_Maybe.showMaybe(Data_Show.showString);
const showMaybeStringDictShow = /* @__PURE__ */ Data_Show.show(showMaybeStringDict);
const showMaybeIntDictShow = /* @__PURE__ */ Data_Show.show(showMaybeIntDict);
const prototypeField = Iris_Effect_Json.fromObject([{
  name: "__proto__",
  value: Iris_Effect_Json.fromString("data")
}]);
const order = Iris_Effect_Json.fromObject([
  {
    name: "product",
    value: Iris_Effect_Json.fromString("p1")
  },
  {
    name: "quantity",
    value: Iris_Effect_Json.fromInt(2 | 0)
  },
  {
    name: "gift",
    value: Iris_Effect_Json.fromBoolean(false)
  },
  {
    name: "tags",
    value: Iris_Effect_Json.fromArray([Iris_Effect_Json.fromString("new"), Iris_Effect_Json.null])
  },
  {
    name: "price",
    value: Iris_Effect_Json.fromNumber(2.5)
  }
]);
export const results = [
  Iris_Effect_Json.stringify(order),
  /* @__PURE__ */ Data_Show.show(/* @__PURE__ */ Data_Show.showArray(showMaybeIntDict))(/* @__PURE__ */ functorArrayDictMap(parsedInt)([
    "2147483647",
    "2147483648",
    "-2147483648",
    "-2147483649",
    "1.5"
  ])),
  Iris_Effect_Json.stringify(prototypeField),
  /* @__PURE__ */ showMaybeStringDictShow(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("__proto__")(prototypeField))(Iris_Effect_Json.toString)),
  /* @__PURE__ */ showMaybeStringDictShow(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("__proto__")(Iris_Effect_Json.fromObject([])))(Iris_Effect_Json.toString)),
  /* @__PURE__ */ showMaybeStringDictShow(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("toString")(Iris_Effect_Json.fromObject([])))(Iris_Effect_Json.toString)),
  /* @__PURE__ */ showMaybeIntDictShow(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("length")(Iris_Effect_Json.fromArray([])))(Iris_Effect_Json.toInt)),
  /* @__PURE__ */ Data_Show.show(/* @__PURE__ */ Data_Maybe.showMaybe(/* @__PURE__ */ Data_Show.showArray(Data_Show.showString)))(/* @__PURE__ */ Data_Functor.map(Data_Maybe.functorMaybe)(/* @__PURE__ */ functorArrayDictMap(Iris_Effect_Json.stringify))(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("tags")(order))(Iris_Effect_Json.toArray))),
  /* @__PURE__ */ Data_Show.show(/* @__PURE__ */ Data_Maybe.showMaybe(Data_Show.showBoolean))(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("gift")(order))(Iris_Effect_Json.toBoolean)),
  /* @__PURE__ */ Data_Show.show(/* @__PURE__ */ Data_Maybe.showMaybe(Data_Show.showNumber))(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("price")(order))(Iris_Effect_Json.toNumber)),
  /* @__PURE__ */ showMaybeIntDictShow(/* @__PURE__ */ bindMaybeDictBind(Iris_Effect_Json.field("price")(order))(Iris_Effect_Json.toInt)),
  describeParse(Iris_Effect_Json.stringify(order)),
  describeParse("[1, 2]"),
  describeParse("{")
];
