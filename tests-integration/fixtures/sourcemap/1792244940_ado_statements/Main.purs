module Main where

import Prelude

combine :: forall f. Apply f => f Int -> f Int -> f Unit -> f Int
combine left right effect = ado
  a <- left
  b <- right
  effect
  in a + b

records :: forall f. Apply f => f { x :: Int } -> f { y :: Int } -> f Int
records left right = ado
  { x } <- left
  { y } <- right
  in x + y
