import { deepStrictEqual } from "node:assert/strict";
import * as Main from "./output/Main/index.js";
import { toCodePointArray } from "./output/Data.String.CodePoints/index.js";

deepStrictEqual(Main.ascii, toCodePointArray("abc"));
deepStrictEqual(Main.astral, toCodePointArray("a\u{1D400}b"));
deepStrictEqual(Main.astral, [0x61, 0x1d400, 0x62]);
deepStrictEqual(Main.loneSurrogate, toCodePointArray("\uD800x"));
deepStrictEqual(Main.loneSurrogate, [0xd800, 0x78]);
deepStrictEqual(Main.empty, toCodePointArray(""));
deepStrictEqual(Main.reexported, toCodePointArray("ab"));
deepStrictEqual(Main.unknown("a\u{1D400}"), toCodePointArray("a\u{1D400}"));
