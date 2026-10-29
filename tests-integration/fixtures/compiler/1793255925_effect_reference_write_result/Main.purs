module Main where

import Prelude

import Effect (Effect)
import Effect.Ref (Ref)
import Effect.Ref as Ref

writeResult :: Ref Int -> Effect Unit
writeResult reference = Ref.write 7 reference

boundWriteResult :: Ref Int -> Effect Unit
boundWriteResult reference = do
  result <- Ref.write 8 reference
  pure result

localWriteResult :: Int -> Effect Unit
localWriteResult value = do
  reference <- Ref.new value
  result <- Ref.write (value + 1) reference
  pure result
