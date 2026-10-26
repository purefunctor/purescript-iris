module Main where

import Data.Tuple (Tuple(..))

letIn :: Tuple Int Int -> Int
letIn t = let Tuple x x = t in x

whereBound :: Tuple Int Int -> Int
whereBound t = x
  where
  Tuple x x = t
