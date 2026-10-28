module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref

statementIf :: Boolean -> Effect Int
statementIf flag = do
  ref <- Ref.new 0
  if flag then Ref.write 1 ref else Ref.write 2 ref
  Ref.modify_ (_ + 10) ref
  Ref.read ref

boundIf :: Boolean -> Effect Int
boundIf flag = do
  ref <- Ref.new 5
  chosen <- if flag then Ref.read ref else pure 0
  Ref.write 7 ref
  pure (chosen + 1)

opaqueBranches :: Boolean -> Effect Unit -> Effect Int -> Effect Int
opaqueBranches flag perform produce = do
  ref <- Ref.new 0
  if flag then perform else Ref.write 1 ref
  result <- if flag then produce else Ref.read ref
  pure (result + 1)
