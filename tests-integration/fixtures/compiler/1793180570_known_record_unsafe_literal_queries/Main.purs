module Main where

import Record.Unsafe (unsafeDelete, unsafeHas)

type Point = { x :: Int, y :: Int }

literalHasX :: Boolean
literalHasX = unsafeHas "x" { x: 1, y: 2 }

literalHasZ :: Boolean
literalHasZ = unsafeHas "z" { x: 1, y: 2 }

hasX :: Point -> Boolean
hasX point = unsafeHas "x" point

literalDeleteY :: Int -> { x :: Int }
literalDeleteY value = unsafeDelete "y" { x: value, y: 2 }

deleteY :: Point -> { x :: Int }
deleteY point = unsafeDelete "y" point
