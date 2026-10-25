let trace = [];

export const observe = label => value => {
  trace.push(label);
  return value;
};

export const readTrace = reset => {
  const result = trace;
  if (reset) trace = [];
  return result;
};
