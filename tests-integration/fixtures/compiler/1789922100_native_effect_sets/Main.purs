module Main where

import Prim.Effect (Abort, class Remove, class Subset, class Union)

data Program :: Effects -> Type -> Type
data Program effects value = Program value

foreign import data Database :: Type
foreign import data Logger :: Type
foreign import data Clock :: Type
foreign import data DatabaseError :: Type
foreign import data ValidationError :: Type

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

discard
  :: forall ignored result left right combined
   . Union left right combined
  => Program left ignored
  -> (ignored -> Program right result)
  -> Program combined result
discard (Program value) continuation =
  case continuation value of
    Program result -> Program result

findUser :: Program [Database] Int
findUser = Program 1

writeLog :: Int -> Program [Logger] Int
writeLog value = Program value

currentTime :: Program [Clock] Int
currentTime = Program 2

databaseFailure :: Program [Abort DatabaseError] Int
databaseFailure = Program 0

validationFailure :: Program [Abort ValidationError] Int
validationFailure = Program 0

catchDatabase
  :: forall value input output
   . Remove (Abort DatabaseError) input output
  => Program input value
  -> Program output value
catchDatabase (Program value) = Program value

catchValidation
  :: forall value input output
   . Remove (Abort ValidationError) input output
  => Program input value
  -> Program output value
catchValidation (Program value) = Program value

provideDatabase
  :: forall value input output
   . Remove Database input output
  => Program input value
  -> Program output value
provideDatabase (Program value) = Program value

restrictEffects
  :: forall value required allowed
   . Subset required allowed
  => Program required value
  -> Program allowed value
restrictEffects (Program value) = Program value

combined = do
  currentTime
  user <- findUser
  writeLog user

reordered :: Program [Clock, Database, Logger] Int
reordered = combined

deduplicated = do
  first <- findUser
  findUser
  succeed first

duplicateAnnotation :: Program [Database, Database] Int
duplicateAnnotation = findUser

expanded :: Program [Database, Logger] Int
expanded = restrictEffects findUser

handled :: Program [Logger, Clock] Int
handled = provideDatabase combined

failures :: Program [Abort DatabaseError, Abort ValidationError] Int
failures = do
  databaseFailure
  validationFailure

partiallyHandled :: Program [Abort ValidationError] Int
partiallyHandled = catchDatabase failures

fullyHandled :: Program [] Int
fullyHandled = catchValidation (catchDatabase failures)

openTail
  :: forall value effects
   . Program [Database, Logger | effects] value
  -> Program [Logger | effects] value
openTail = provideDatabase
