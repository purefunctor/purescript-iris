module Main where

data Maybe a = Just a | Nothing

newtype Box a = Box a

data Pair a b = Pair a b

infixr 6 Pair as /\

operatorGuard :: Maybe Int -> Int
operatorGuard = case _ of
  Just n | a /\ _ <- n /\ n -> a
  Nothing -> 0

operatorChainGuard :: Maybe Int -> Pair Int Int
operatorChainGuard = case _ of
  Just n | a /\ b /\ _ <- n /\ n /\ n -> a /\ b
  Nothing -> 0 /\ 0

nestedOperatorGuard :: Maybe Int -> Int
nestedOperatorGuard = case _ of
  Just n | Box (a /\ _) <- Box (n /\ n) -> a
  Nothing -> 0

refutableOperatorGuard = case _ of
  Just n | _ /\ Just b <- n /\ Just n -> b
  Nothing -> 0
