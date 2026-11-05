const trace = [];

export const step = label => value => {
  trace.push(`construct:${label}:${value}`);
  return () => {
    trace.push(`run:${label}`);
    return value + 2;
  };
};

export const complete = value => () => trace.push(`result:${value}`);
export const failure = error => () => trace.push(`failure:${error}`);
export const mark = label => value => {
  trace.push(label);
  return value;
};
export const readTrace = () => [...trace];
export const resetTrace = () => { trace.length = 0; };
