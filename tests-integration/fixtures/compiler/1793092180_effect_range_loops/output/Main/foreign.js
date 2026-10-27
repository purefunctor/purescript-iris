const trace = [];

export const resetTrace = () => {
  trace.length = 0;
};

export const readTrace = () => trace.slice();

export const traced = label => value => {
  trace.push(label);
  return value;
};
