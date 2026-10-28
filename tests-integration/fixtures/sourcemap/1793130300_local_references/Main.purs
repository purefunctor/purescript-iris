module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref

labelled :: Effect String
labelled = do
  ref <- Ref.new 1
  label <- Ref.modify' (\state -> { state: state + 1, value: show state }) ref
  Ref.modify_ (_ * 2) ref
  pure label

written :: Int -> Int
written start = ST.run do
  ref <- STRef.new start
  value <- STRef.write (start + 1) ref
  STRef.modify (_ + value) ref
