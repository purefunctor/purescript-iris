module Main where

import Prelude

import Data.Maybe (Maybe(..))

first :: Maybe Int -> Maybe Int -> Int
first left right
  | Just value <- left = value
  | (Just value) <- right = value
  | otherwise = 0
