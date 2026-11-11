import { strictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import * as Ref from "./output/Effect.Ref/index.js";

const literal = (value) => Main.Literal(value);
const add = (left, right) => Main.Add(left)(right);
const parenthesised = (expression) => Main.Parenthesised(expression);
const sum = (expressions) =>
  Main.Sum(expressions.reduceRight((rest, expression) => Main.Cons(expression)(rest), Main.Nil));

let nested = literal(7);
for (let depth = 0; depth < 100_000; depth++) {
  nested = parenthesised(nested);
}
strictEqual(Main.evaluate(nested), 7);

const terms = Array.from({ length: 100_000 }, (_, index) => literal(index % 3));
strictEqual(Main.evaluate(sum(terms)), 99_999);

const expression = parenthesised(sum([add(literal(1), literal(2)), parenthesised(sum([literal(3), literal(4)]))]));
strictEqual(Main.evaluate(expression), 10);
strictEqual(Main.sumTerms(5)(Main.Nil), 5);

const counter = Ref.new(5)();
strictEqual(Main.countToZero(counter)(), 6);
strictEqual(Main.countToZero(counter)(), 7);

const negative = Ref.new(-3)();
strictEqual(Main.countToZero(negative)(), -2);
strictEqual(Ref.read(negative)(), 0);

const skipped = Ref.new(-2)();
strictEqual(Main.skipToZero(skipped)(100_000)(), -1);
strictEqual(Ref.read(skipped)(), 0);
