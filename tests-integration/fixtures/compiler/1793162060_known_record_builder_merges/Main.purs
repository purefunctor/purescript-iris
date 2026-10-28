module Main where

import Prelude

import Record.Builder as Builder
import Type.Proxy (Proxy(..))

merge :: { x :: Int, y :: String } -> { x :: Int, y :: String, z :: Boolean }
merge record = Builder.build (Builder.merge { x: 0, z: true }) record

union :: { y :: String } -> { y :: String, z :: Boolean }
union record = Builder.build (Builder.union { z: true }) record

disjointUnion :: { y :: String } -> { y :: String, z :: Boolean }
disjointUnion record = Builder.build (Builder.disjointUnion { z: true }) record

records :: { x :: Int } -> { y :: String } -> { x :: Int, y :: String }
records left right = Builder.build (Builder.merge left) right

literals :: { x :: Int, y :: String }
literals = Builder.build (Builder.merge { x: 0 }) { y: "y", x: 1 }

insertThenMerge :: { y :: String } -> { y :: String, z :: Boolean, x :: Int }
insertThenMerge record =
  Builder.build (Builder.insert (Proxy :: _ "z") true >>> Builder.merge { x: 0 }) record
