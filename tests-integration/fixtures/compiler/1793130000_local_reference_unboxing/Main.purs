module Main where

import Prelude

import Control.Monad.ST as ST
import Control.Monad.ST.Ref as STRef
import Effect (Effect)
import Effect.Ref as Ref

refs :: Effect Int
refs = do
  ref <- Ref.new 0
  Ref.write 1 ref
  Ref.modify_ (_ + 1) ref
  Ref.read ref

branches :: Boolean -> Effect String
branches flag = do
  ref <- Ref.new "start"
  if flag then do
    Ref.write "then" ref
    map (_ <> "!") (Ref.read ref)
  else do
    Ref.modify_ (_ <> "-else") ref
    Ref.read ref

stateWrites :: Int -> Array Int
stateWrites start = ST.run do
  ref <- STRef.new start
  written <- STRef.write (start * 2) ref
  modified <- STRef.modify (_ + 1) ref
  final <- STRef.read ref
  pure [ written, modified, final ]
