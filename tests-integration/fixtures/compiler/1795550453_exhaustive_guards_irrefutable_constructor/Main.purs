module Main where

data Maybe a = Just a | Nothing

newtype Box a = Box a

data Pair a b = Pair a b

newtypeGuard :: Maybe Int -> Int
newtypeGuard = case _ of
  Just n | Box m <- Box n -> m
  Nothing -> 0

singleConstructorGuard :: Maybe Int -> Int
singleConstructorGuard = case _ of
  Just n | Pair a _ <- Pair n n -> a
  Nothing -> 0

recordGuard :: Maybe Int -> Int
recordGuard = case _ of
  Just n | { value } <- { value: n } -> value
  Nothing -> 0

nestedGuard :: Maybe Int -> Pair Int Int
nestedGuard = case _ of
  Just n | Box (Pair a { value }) <- Box (Pair n { value: n }) -> Pair a value
  Nothing -> Pair 0 0

refutableGuard = case _ of
  Just n | Box (Just m) <- Box (Just n) -> m
  Nothing -> 0
