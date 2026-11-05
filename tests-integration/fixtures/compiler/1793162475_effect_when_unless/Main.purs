module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref

foreign import record :: String -> Effect Unit
foreign import takeLog :: Effect (Array String)

statementWhen :: Boolean -> Effect Int
statementWhen flag = do
  ref <- Ref.new 0
  when flag (Ref.write 5 ref)
  unless flag (Ref.modify_ (_ + 1) ref)
  Ref.read ref

finalWhen :: Boolean -> Effect Unit
finalWhen flag = do
  record "before"
  when flag (record "inside")

stateUnless :: Int -> Int
stateUnless count = ST.run do
  ref <- STRef.new count
  unless (count > 0) (void (STRef.write 0 ref))
  STRef.read ref
