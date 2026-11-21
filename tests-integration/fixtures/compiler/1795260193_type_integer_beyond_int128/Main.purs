module Main where

import Prim.Int (class Add, class Mul)
import Type.Proxy (Proxy(..))

literalBeyondInt128 :: Proxy 170141183460469231731687303715884105728
literalBeyondInt128 = Proxy

addBeyondInt128 :: forall sum. Add 170141183460469231731687303715884105727 1 sum => Proxy sum
addBeyondInt128 = Proxy

mulBeyondInt128
  :: forall product
   . Mul 100_000_000_000_000_000_000 100_000_000_000_000_000_000 product
  => Proxy product
mulBeyondInt128 = Proxy

forceSolve =
  { addBeyondInt128
  , mulBeyondInt128
  }
