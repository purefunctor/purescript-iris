export const marks = [];

export const mark = (label) => (value) => {
  marks.push(label);
  return value;
};

export const peek = (reference) => reference.value;

export const poke = (reference) => (value) => {
  reference.value = value;
};
