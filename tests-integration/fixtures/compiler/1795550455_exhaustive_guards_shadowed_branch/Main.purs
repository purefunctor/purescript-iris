module Main where

data Maybe a = Just a | Nothing

newtype Box a = Box a

shadowedBranch :: Maybe Int -> Int
shadowedBranch = case _ of
  Just n | Box m <- Box n -> m
  Just n -> n
  Nothing -> 0
