module Main where

import Prim.Effect (Abort, class Subset, class Union)

data Program :: Effects -> Type -> Type
data Program effects value = Program value

foreign import data Database :: Type
foreign import data Logger :: Type
foreign import data DatabaseError :: Type
foreign import data ValidationError :: Type

instance fabricatedSubset :: Subset [Database, Logger] []

succeed :: forall value. value -> Program [] value
succeed value = Program value

bind
  :: forall value result left right combined
   . Union left right combined
  => Program left value
  -> (value -> Program right result)
  -> Program combined result
bind (Program value) continuation =
  case continuation value of
    Program result -> Program result

writeLog :: Int -> Program [Logger] Int
writeLog value = Program value

databaseFailure :: Program [Abort DatabaseError] Int
databaseFailure = Program 0

validationFailure :: Program [Abort ValidationError] Int
validationFailure = Program 0

restrictEffects
  :: forall value required allowed
   . Subset required allowed
  => Program required value
  -> Program allowed value
restrictEffects (Program value) = Program value

missingEffect :: Program [] Int
missingEffect =
  restrictEffects do
    value <- (Program 1 :: Program [Database] Int)
    logged <- writeLog value
    succeed logged

anotherMissingEffect :: Program [Logger] Int
anotherMissingEffect =
  restrictEffects do
    logged <- writeLog 2
    value <- (Program logged :: Program [Database] Int)
    succeed value

uncaughtAborts :: Program [] Int
uncaughtAborts =
  restrictEffects do
    value <- databaseFailure
    validation <- validationFailure
    succeed validation

finalAbort :: Program [] Int
finalAbort =
  restrictEffects do
    value <- succeed 1
    databaseFailure

localBudget :: Program [] Int
localBudget =
  let
    local :: Program [] Int
    local =
      restrictEffects do
        value <- databaseFailure
        succeed value
  in local

siblingBudgets
  :: { allowed :: Program [Abort DatabaseError] Int
     , rejected :: Program [] Int
     }
siblingBudgets =
  { allowed: do
      value <- databaseFailure
      succeed value
  , rejected:
      restrictEffects do
        value <- databaseFailure
        succeed value
  }
