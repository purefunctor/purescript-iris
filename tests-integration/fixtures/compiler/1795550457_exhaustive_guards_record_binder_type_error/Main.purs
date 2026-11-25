module Main where

data Maybe a = Just a | Nothing

nonRecordGuard :: Maybe Int -> Int
nonRecordGuard = case _ of
  Just n | { value: Just v } <- n -> v
  Nothing -> 0

unexpectedLabelGuard :: Maybe { other :: Int } -> Int
unexpectedLabelGuard = case _ of
  Just r | { value: Just v } <- r -> v
  Nothing -> 0
