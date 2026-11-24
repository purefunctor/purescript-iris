module Main where

data Maybe a = Just a | Nothing

unresolvedConstructorGuard = case _ of
  Just n | Unknown m <- n -> m
  Nothing -> 0

unresolvedOperatorGuard = case _ of
  Just n | a <+> _ <- n -> a
  Nothing -> 0
