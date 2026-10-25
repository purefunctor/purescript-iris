module Main where

import Prelude

import Effect (Effect)

sharedEffect :: Int -> Int -> Effect Int
sharedEffect left right =
  let total = left + right
  in pure unit >>= \_ -> pure total

runningBind :: Int -> Int -> Effect Int
runningBind left right = do
  _ <- pure unit
  let total = left + right
  pure total
