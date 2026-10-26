module Main where

import Prelude

import Data.Tuple (Tuple(..))
import Effect (Effect)

doBind :: Effect (Tuple Int Int) -> Effect Int
doBind action = do
  Tuple x x <- action
  pure x

adoBind :: Effect (Tuple Int Int) -> Effect Int
adoBind action = ado
  Tuple x x <- action
  in x

successiveBinds :: Effect Int -> Effect Int
successiveBinds action = do
  x <- action
  x <- pure (x + 1)
  pure x
