module Main where

import Prelude

import Data.Tuple (Tuple(..))

withinGuard :: Tuple Int Int -> Int
withinGuard t
  | Tuple x x <- t = x
  | otherwise = 0

shadowsArgument :: Int -> Tuple Int Int -> Int
shadowsArgument x t
  | Tuple x _ <- t = x
  | otherwise = 0
