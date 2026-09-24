module Main where

import Prelude
import Data.Lens.Iso (iso)

test :: Int
test = (iso identity identity identity :: Int -> Int) 42

partial :: Int -> Int
partial = iso (_ + 3) (_ * 2) (_ + 5)

type Transform a = Show a => a -> String

after :: forall a. String -> Transform a
after prefix value = prefix <> show value

before :: forall a. Show a => String -> a -> String
before prefix value = prefix <> show value

direct :: String
direct = after "value=" 42

applied :: Int -> String
applied = after "partial="

dictionaryFirst :: String
dictionaryFirst = before "before=" 17

eta :: forall a. String -> Transform a
eta = after

branches :: forall a. Boolean -> Transform a
branches true value = "true=" <> show value
branches false = after "false="

interleaved :: forall a b. Show a => a -> Eq b => b -> b -> String
interleaved value left right = if left == right then show value else "different"

local :: String
local = nested "local=" 13
  where
  nested :: forall a. String -> Transform a
  nested prefix value = prefix <> show value
