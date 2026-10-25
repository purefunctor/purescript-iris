module Main where

import Prelude

import Effect (Effect)

nestedBind :: Int -> Int -> Effect Int
nestedBind left right =
  (let total = left + right in pure unit >>= \_ -> pure total) >>= \value -> pure value

nestedMap :: Int -> Int -> Effect Int
nestedMap left right =
  identity <$> (let total = left + right in pure unit >>= \_ -> pure total)

nestedApply :: Int -> Int -> Effect Int
nestedApply left right =
  pure identity <*> (let total = left + right in pure unit >>= \_ -> pure total)
