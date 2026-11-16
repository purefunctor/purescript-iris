module Main where

import Prelude
import Data.Newtype (class Newtype, unwrap)

newtype Attached a = Attached a
  derive (Newtype)

data Plain :: Type -> Type
data Plain a = Plain a
type role Plain nominal

newtype Standalone a = Standalone a
type role Standalone representational
derive instance Newtype (Standalone a) _

foreign import data Foreign :: Type -> Type
type role Foreign representational

result = unwrap (Attached 41) + unwrap (Standalone 1)
