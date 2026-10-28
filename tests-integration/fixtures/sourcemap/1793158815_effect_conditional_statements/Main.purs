module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref

data Choice = First | Second Int

chosen :: Boolean -> Choice -> Effect Int
chosen flag choice = do
  ref <- Ref.new 0
  if flag then Ref.write 1 ref else Ref.modify_ (_ + 2) ref
  value <- case choice of
    First -> Ref.read ref
    Second count -> pure count
  when flag (Ref.write value ref)
  Ref.read ref
