module Main where

import Prelude

import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref

type Results =
  { written :: Unit
  , modified :: Int
  , modifiedUnit :: Unit
  , modifiedValue :: String
  , final :: Int
  }

results :: Effect Results
results = do
  ref <- Ref.new 1
  written <- Ref.write 2 ref
  modified <- Ref.modify (_ + 10) ref
  modifiedUnit <- Ref.modify_ (_ * 2) ref
  modifiedValue <- Ref.modify' (\state -> { state: state + 1, value: show state }) ref
  final <- Ref.read ref
  pure { written, modified, modifiedUnit, modifiedValue, final }

increment :: Ref Int -> Effect Unit
increment ref = Ref.modify_ (\count -> count + 1) ref
