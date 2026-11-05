export function console(environment) {
  return environment.logger;
}
export function http(environment) {
  return environment.network;
}
export function misleading(environment) {
  return environment.http;
}
export function aliased(environment) {
  return environment.renamed;
}
export const consoleValue = console({
  logger: 19 | 0,
  network: 23 | 0
});
export const httpValue = http({
  logger: 19 | 0,
  network: 23 | 0
});
export const misleadingValue = misleading({
  console: 9 | 0,
  http: 42 | 0
});
export const aliasedValue = aliased({ renamed: 51 | 0 });
