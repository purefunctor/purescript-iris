let log = [];

export const observe = value => {
  log.push(`observe ${value}`);
  return value;
};

export const takeLog = () => {
  const taken = log;
  log = [];
  return taken;
};
