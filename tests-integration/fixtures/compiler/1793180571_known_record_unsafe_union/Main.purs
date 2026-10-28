module Main where

import Data.Function.Uncurried (runFn2)
import Record.Unsafe.Union (unsafeUnion, unsafeUnionFn)

type Union = { x :: Int, y :: Int, z :: Int }

literalUnion :: Int -> Union
literalUnion value = unsafeUnion { x: value, y: 2 } { y: 20, z: 30 }

literalUnionFn :: Int -> Union
literalUnionFn value = runFn2 unsafeUnionFn { x: 1, y: value } { y: 20, z: 30 }

unionWithLiteral :: { y :: Int, z :: Int } -> Union
unionWithLiteral right = unsafeUnion { x: 1, y: 2 } right
