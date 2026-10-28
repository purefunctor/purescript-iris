module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref

foreign import record :: String -> Effect Int
foreign import takeLog :: Effect (Array String)

statements :: Effect Int
statements = do
  ref <- Ref.new 0
  void (Ref.modify (_ + 1) ref)
  void $ Ref.modify (_ + 2) ref
  Ref.read ref

final :: Effect Unit
final = do
  _ <- record "first"
  void (record "second")

nested :: Effect Unit
nested = void do
  _ <- record "inner"
  record "last"

discarded :: Effect Int -> Effect Unit
discarded action = void action

stateWrite :: Unit
stateWrite = ST.run do
  ref <- STRef.new 0
  void (STRef.write 5 ref)
