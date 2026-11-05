export const push = values => value => () => {
  values.push(value);
};

export const poke = values => index => value => () => {
  values[index] = value;
};
