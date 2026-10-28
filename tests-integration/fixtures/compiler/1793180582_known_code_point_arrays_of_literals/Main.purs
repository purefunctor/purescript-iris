module Main where

import Data.String (CodePoint)
import Data.String as String
import Data.String.CodePoints (toCodePointArray)

ascii :: Array CodePoint
ascii = toCodePointArray "abc"

astral :: Array CodePoint
astral = toCodePointArray "a𝐀b"

loneSurrogate :: Array CodePoint
loneSurrogate = toCodePointArray "\xD800x"

empty :: Array CodePoint
empty = toCodePointArray ""

reexported :: Array CodePoint
reexported = String.toCodePointArray "ab"

unknown :: String -> Array CodePoint
unknown string = toCodePointArray string
