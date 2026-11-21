module Main where

import Data.Symbol (class IsSymbol, reflectSymbol)
import Prim.Int (class Add, class Compare, class Mul, class ToString)
import Prim.Ordering (GT, LT)
import Type.Proxy (Proxy(..))

addBeyondInt64 :: forall sum. Add 99999999999999999999 1 sum => Proxy sum
addBeyondInt64 = Proxy

subtractBeyondInt64 :: forall right. Add 1 right 99999999999999999999 => Proxy right
subtractBeyondInt64 = Proxy

mulBeyondInt64 :: forall product. Mul 10_000_000_000 10_000_000_000 product => Proxy product
mulBeyondInt64 = Proxy

negativeBeyondInt64 :: forall product. Mul (-99999999999999999999) 2 product => Proxy product
negativeBeyondInt64 = Proxy

hexadecimalBeyondInt64 :: forall string. ToString 0xffffffffffffffff string => Proxy string
hexadecimalBeyondInt64 = Proxy

compareBeyondInt64 :: Compare 99999999999999999999 9223372036854775807 GT => Proxy GT
compareBeyondInt64 = Proxy

compareBelowInt64 :: Compare (-99999999999999999999) (-9223372036854775808) LT => Proxy LT
compareBelowInt64 = Proxy

digits :: forall @n string. ToString n string => IsSymbol string => String
digits = reflectSymbol (Proxy :: Proxy string)

reflectedBeyondInt64 :: String
reflectedBeyondInt64 = digits @99999999999999999999

reflectedMaximum :: String
reflectedMaximum = digits @170141183460469231731687303715884105727

reflectedMinimum :: String
reflectedMinimum = digits @(-170141183460469231731687303715884105728)

forceSolve =
  { addBeyondInt64
  , subtractBeyondInt64
  , mulBeyondInt64
  , negativeBeyondInt64
  , hexadecimalBeyondInt64
  , compareBeyondInt64
  , compareBelowInt64
  }
