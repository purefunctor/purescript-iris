module Main where

data Maybe a = Just a | Nothing

newtype Box a = Box a

totalGuardAfterCatchAll :: Maybe Int -> Int
totalGuardAfterCatchAll = case _ of
  _ -> 0
  Just n | Box m <- Box n -> m

conditionGuardAfterCatchAll :: Maybe Boolean -> Int
conditionGuardAfterCatchAll = case _ of
  _ -> 0
  Just b | b -> 1

refutableGuardAfterCatchAll :: Maybe (Maybe Int) -> Int
refutableGuardAfterCatchAll = case _ of
  _ -> 0
  Just m | Just n <- m -> n

conditionGuardAfterSamePattern :: Maybe Boolean -> Int
conditionGuardAfterSamePattern = case _ of
  Just _ -> 0
  Just b | b -> 1
  Nothing -> 2

conditionGuardEquation :: Maybe Boolean -> Int
conditionGuardEquation _ = 0
conditionGuardEquation (Just b) | b = 1
