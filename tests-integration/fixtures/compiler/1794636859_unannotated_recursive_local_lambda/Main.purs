module Main where

import Prelude

call :: forall f. (Unit -> f) -> f
call k = k unit

f :: Int -> Int
f = go
  where
  go = call \_ -> go
