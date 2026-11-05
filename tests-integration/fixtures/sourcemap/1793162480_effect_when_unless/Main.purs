module Main where

import Prelude

import Effect (Effect)
import Effect.Ref as Ref

counter :: Boolean -> Effect Int
counter flag = do
  count <- Ref.new 0
  when flag do
    Ref.modify_ (_ + 1) count
  unless flag (Ref.write 10 count)
  Ref.read count
