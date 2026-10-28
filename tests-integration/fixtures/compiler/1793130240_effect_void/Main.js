let log = [];

export const record = label => () => {
  log.push(label);
  return log.length;
};

export const takeLog = () => {
  const taken = log;
  log = [];
  return taken;
};
