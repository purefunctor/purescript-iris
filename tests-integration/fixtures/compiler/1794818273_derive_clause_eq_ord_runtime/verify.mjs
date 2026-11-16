import assert from 'node:assert/strict';
import { equal, unequal, ordered, control } from './output/Main/index.js';

assert.equal(equal, true);
assert.equal(unequal, false);
assert.equal(ordered, true);
assert.equal(control, true);
