export function singleFieldUpdate(section9) {
  return {
    ...section9,
    a: 1 | 0
  };
}
export function multipleFieldUpdate(section17) {
  return {
    ...section17,
    a: 2 | 0,
    b: true,
    c: "three"
  };
}
export function nestedRecordUpdate(section29) {
  return {
    ...section29,
    a: {
      ...section29.a,
      b: {
        ...section29.a.b,
        c: 42 | 0
      }
    }
  };
}
export function updateWithValueSection(section41) {
  return {
    ...section41,
    a: (section46) => add(section46)(1 | 0)
  };
}
export function updateWithSectionInteraction(section54) {
  return (section57) => {
    return {
      ...section54,
      a: section57
    };
  };
}
export function polymorphicRecordUpdate(section62) {
  return {
    ...section62,
    foo: 0 | 0
  };
}
export function multipleSectionsInteraction(section82) {
  return (section85) => {
    return (section87) => {
      return {
        ...section82,
        x: section85,
        y: section87
      };
    };
  };
}
export function nestedSectionInteraction(section92) {
  return (section97) => {
    return {
      ...section92,
      a: {
        ...section92.a,
        b: section97
      }
    };
  };
}
export function mixedSections(section102) {
  return {
    ...section102,
    a: (section106) => add(section106)(1 | 0),
    b: 2 | 0
  };
}
export function recordAccessSectionUpdate(section116) {
  return {
    ...section116,
    a: (section120) => section120.b
  };
}
export function concreteRecordUpdateSection(section132) {
  return {
    ...{ a: 1 | 0 },
    a: section132
  };
}
export function map(f) {
  return (x) => {
    return [];
  };
}
export function add(x) {
  return (y) => {
    return x;
  };
}
export const higherOrderContext = /* @__PURE__ */ (() => {
  return map((section74) => {
    return {
      ...section74,
      x: 10 | 0
    };
  });
})();
