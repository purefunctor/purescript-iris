export const marks = [];

export const mark = (label) => (value) => {
  marks.push(label);
  return value;
};
