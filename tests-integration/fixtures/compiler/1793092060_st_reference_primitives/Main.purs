module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef

type Results =
  { written :: Int
  , modified :: Int
  , modifiedValue :: String
  , final :: Int
  }

results :: Results
results = ST.run do
  ref <- STRef.new 1
  written <- STRef.write 2 ref
  modified <- STRef.modify (_ + 10) ref
  modifiedValue <- STRef.modify' (\state -> { state: state * 2, value: show state }) ref
  final <- STRef.read ref
  pure { written, modified, modifiedValue, final }
