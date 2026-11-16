import assert from 'node:assert/strict';
import * as Main from './output/Main/index.js';

assert.equal(Main.mapped, true);
assert.equal(Main.folded, '38');
assert.equal(Main.traversed, true);
assert.equal(Main.rejected, true);
assert.equal(Main.equal, true);
assert.equal(Main.unequal, false);
assert.equal(Main.ordered, true);
