import { deepStrictEqual } from "node:assert/strict";
import { results } from "./output/Main/index.js";

deepStrictEqual(results, [
  '{"product":"p1","quantity":2,"gift":false,"tags":["new",null],"price":2.5}',
  "[(Just 2147483647),Nothing,(Just -2147483648),Nothing,Nothing]",
  '{"__proto__":"data"}',
  '(Just "data")',
  "Nothing",
  "Nothing",
  "Nothing",
  '(Just ["\\"new\\"","null"])',
  "(Just false)",
  "(Just 2.5)",
  "Nothing",
  "product p1",
  "no product",
  "invalid",
]);
