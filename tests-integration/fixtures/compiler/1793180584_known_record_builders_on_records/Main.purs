module Main where

import Prelude

import Record.Builder (Builder, build, delete, insert, merge, modify)
import Type.Proxy (Proxy(..))

type Point = { x :: Int, y :: Int }

withZ :: Int -> Point -> { x :: Int, y :: Int, z :: Int }
withZ value point = build (insert (Proxy :: _ "z") value) point

shifted :: Point -> Point
shifted point = build (modify (Proxy :: _ "x") (_ + 1) >>> modify (Proxy :: _ "y") (_ * 2)) point

insertedThenModified :: Point -> { x :: Int, y :: Int, z :: Int }
insertedThenModified point = build (insert (Proxy :: _ "z") 5 >>> modify (Proxy :: _ "z") (_ + 1)) point

mergedOnto :: Point -> { x :: Int, y :: Int, z :: Int }
mergedOnto point = build (merge { y: 20, z: 30 }) point

withoutY :: Point -> { x :: Int }
withoutY point = build (delete (Proxy :: _ "y")) point

unknownBuilder :: Builder Point Point -> Point -> Point
unknownBuilder builder point = build builder point
