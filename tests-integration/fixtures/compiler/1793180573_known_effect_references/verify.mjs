import { strictEqual } from "node:assert/strict";
import * as Effect_Ref from "./output/Effect.Ref/index.js";
import * as Main from "./output/Main/index.js";

const newCounter = Main.newCounter(1);
const counter = newCounter();
const anotherCounter = newCounter();
strictEqual(Effect_Ref.read(counter)(), 1);
strictEqual(Effect_Ref.read(anotherCounter)(), 1);
Effect_Ref.write(2)(counter)();
strictEqual(Effect_Ref.read(anotherCounter)(), 1);

const libraryCounter = Effect_Ref.new(5)();
const readCounter = Main.readCounter(libraryCounter);
Effect_Ref.write(6)(libraryCounter)();
strictEqual(readCounter(), 6);

const resetCounter = Main.resetCounter(libraryCounter);
strictEqual(Effect_Ref.read(libraryCounter)(), 6);
resetCounter();
strictEqual(Effect_Ref.read(libraryCounter)(), 0);

const incrementCounter = Main.incrementCounter(libraryCounter);
strictEqual(Effect_Ref.read(libraryCounter)(), 0);
Effect_Ref.write(41)(libraryCounter)();
strictEqual(incrementCounter(), 42);
strictEqual(incrementCounter(), 43);
strictEqual(Effect_Ref.read(libraryCounter)(), 43);

const state = { counter: Effect_Ref.new(1)() };
const addToField = Main.addToField(5)(state);
strictEqual(Effect_Ref.read(state.counter)(), 1);
strictEqual(addToField(), 6);
strictEqual(addToField(), 11);

strictEqual(Main.countTwice(3)(), 80);
