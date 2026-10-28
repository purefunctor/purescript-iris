import { deepStrictEqual, strictEqual } from "node:assert/strict";
import * as Effect_Ref from "./output/Effect.Ref/index.js";
import * as Main from "./output/Main/index.js";

const reference = Effect_Ref.new(1)();
strictEqual(Main.addToReference.length, 2);
strictEqual(Main.addToReference(reference, 2), 3);
strictEqual(Effect_Ref.read(reference)(), 3);

const calls = [];
const recordCall = (target, amount) => {
  calls.push(amount);
  return Effect_Ref.read(target)() + amount;
};
const addFive = Main.addFive(recordCall)(reference);
deepStrictEqual(calls, []);
strictEqual(addFive(), 8);
strictEqual(addFive(), 8);
deepStrictEqual(calls, [5, 5]);
strictEqual(Main.addFive(Main.addToReference)(reference)(), 8);
strictEqual(Effect_Ref.read(reference)(), 8);

const messages = [];
const logTwice = Main.logTwice((message) => {
  messages.push(message);
})("hello");
deepStrictEqual(messages, []);
logTwice();
deepStrictEqual(messages, ["hello", "hello!"]);
