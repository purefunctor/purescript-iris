module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref

foreign import observe :: Int -> Int
foreign import takeLog :: Effect (Array String)

finalRead :: Effect Int
finalRead = do
  ref <- Ref.new 0
  void do
    Ref.write 3 ref
    Ref.read ref
  Ref.read ref

finalPure :: Int -> Effect Int
finalPure count = do
  ref <- Ref.new count
  void do
    Ref.modify_ (_ + 1) ref
    pure count
  void do
    Ref.modify_ (_ * 2) ref
    pure (observe count)
  Ref.read ref

finalMap :: Effect Int
finalMap = do
  ref <- Ref.new 1
  void do
    Ref.write 4 ref
    map observe (Ref.read ref)
  Ref.read ref
