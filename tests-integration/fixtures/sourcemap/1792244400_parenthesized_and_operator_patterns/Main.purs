module Main where

import Prelude

data Pair = Pair Int Int

infixr 6 Pair as :&

sum :: Pair -> Int
sum pair =
  let
    (Pair left right) = pair
  in
    left + right

difference :: Pair -> Int
difference pair = case pair of
  (Pair left right) -> left - right

first :: Pair -> Int
first pair = case pair of
  left :& _ -> left
