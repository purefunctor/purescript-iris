module Main where

import Prelude
import Data.Generic.Rep (class Generic, from, to)
import Data.Newtype (class Newtype, unwrap, wrap)

data Phantom :: forall k. k -> Type
data Phantom a = Phantom Int
  derive (Generic)

newtype Tagged :: forall k. k -> Type -> Type
newtype Tagged tag a = Tagged a
  derive (Newtype, Generic)

roundtrip :: Phantom "tag" -> Phantom "tag"
roundtrip value = to (from value)

result :: Int
result = unwrap (wrap 42 :: Tagged "tag" Int)

observed = case roundtrip (Phantom 37) of
  Phantom value -> value
