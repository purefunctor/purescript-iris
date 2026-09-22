if (process.argv[2] === "--") {
  process.argv.splice(2, 1);
}
const target = await import(process.argv[1]);
if (typeof target.main !== "function") {
  throw new Error(`Module does not export a main function: ${process.argv[1]}`);
}
await target.main();
