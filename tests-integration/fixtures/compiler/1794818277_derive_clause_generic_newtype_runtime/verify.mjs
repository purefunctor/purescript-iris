import assert from 'node:assert/strict';
import { result, observed } from './output/Main/index.js';

assert.equal(result, 42);
assert.equal(observed, 37);
