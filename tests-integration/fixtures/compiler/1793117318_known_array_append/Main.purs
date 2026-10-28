module Main where

import Prelude

arrayAppend :: Array Int -> Array Int -> Array Int
arrayAppend left right = left <> right

arrayAppendChain :: Array Int -> Array Int -> Array Int -> Array Int
arrayAppendChain first second third = first <> second <> third

arrayAppendGrouped :: Array Int -> Array Int -> Array Int -> Array Int -> Array Int
arrayAppendGrouped first second third fourth = (first <> second) <> (third <> fourth)

arrayAppendLiterals :: Array Int -> Array Int
arrayAppendLiterals middle = [1, 2] <> middle <> [3] <> []

literalAppend :: Array Int
literalAppend = [1] <> [2, 3]

partiallyAppliedAppend :: Array Int -> Array Int
partiallyAppliedAppend = append [0]
