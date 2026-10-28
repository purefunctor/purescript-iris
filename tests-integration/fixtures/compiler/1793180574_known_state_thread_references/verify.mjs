import { strictEqual } from "node:assert/strict";
import * as STRef from "./output/Control.Monad.ST.Internal/index.js";
import * as Main from "./output/Main/index.js";

const newCounter = Main.newCounter(1);
const counter = newCounter();
const anotherCounter = newCounter();
strictEqual(STRef.read(counter)(), 1);
STRef.write(2)(counter)();
strictEqual(STRef.read(anotherCounter)(), 1);

const libraryCounter = STRef.new(5)();
const readCounter = Main.readCounter(libraryCounter);
STRef.write(6)(libraryCounter)();
strictEqual(readCounter(), 6);

const resetCounter = Main.resetCounter(libraryCounter);
strictEqual(STRef.read(libraryCounter)(), 6);
strictEqual(resetCounter(), 0);
strictEqual(STRef.read(libraryCounter)(), 0);

const incrementCounter = Main.incrementCounter(libraryCounter);
STRef.write(41)(libraryCounter)();
strictEqual(incrementCounter(), 42);
strictEqual(incrementCounter(), 43);
strictEqual(STRef.read(libraryCounter)(), 43);

strictEqual(Main.countTwice(3), 84);
