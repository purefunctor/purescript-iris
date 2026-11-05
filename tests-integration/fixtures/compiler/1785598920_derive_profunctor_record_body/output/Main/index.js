export const RecordPro = ($value0) => ({
  tag: "RecordPro",
  _1: $value0
});
export const profunctorRecordPro = /* @__PURE__ */ (() => {
  return { dimap: (firstFunction) => {
    return (secondFunction) => {
      return (value) => {
        if (value.tag === "RecordPro") {
          const { _1: field0 } = value;
          return {
            tag: "RecordPro",
            _1: {
              ...field0,
              consume: (argument) => field0.consume(firstFunction(argument)),
              produce: secondFunction(field0.produce)
            }
          };
        }
        throw new Error("Pattern match failure");
      };
    };
  } };
})();
