module Main where

import Record.Unsafe (unsafeGet, unsafeSet)

type Point = { x :: Int, y :: Int }

readX :: Point -> Int
readX point = unsafeGet "x" point

readSpacedLabel :: { "first name" :: String } -> String
readSpacedLabel person = unsafeGet "first name" person

setX :: Int -> Point -> Point
setX value point = unsafeSet "x" value point

addZ :: Int -> Point -> { x :: Int, y :: Int, z :: Int }
addZ value point = unsafeSet "z" value point

readLabel :: String -> Point -> Int
readLabel label point = unsafeGet label point
