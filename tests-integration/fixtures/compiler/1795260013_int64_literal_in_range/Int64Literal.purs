module Int64Literal (int64Digits, class Int64Range, class InRange) where

import Data.Symbol (class IsSymbol, reflectSymbol)
import Prim.Int (class Compare, class ToString)
import Prim.Ordering (GT, LT, Ordering)
import Prim.TypeError (class Fail, Beside, Doc, Quote, Text)
import Type.Proxy (Proxy(..))

int64Digits :: forall @n digits. Int64Range n => ToString n digits => IsSymbol digits => String
int64Digits = reflectSymbol (Proxy :: Proxy digits)

class Int64Range (n :: Int)

instance
  ( Compare n (-9223372036854775808) fromBottom
  , Compare n 9223372036854775807 fromTop
  , InRange fromBottom fromTop n (Text "Int64 (-9223372036854775808 to 9223372036854775807)")
  ) =>
  Int64Range n

class InRange (fromBottom :: Ordering) (fromTop :: Ordering) (n :: Int) (range :: Doc)

instance
  Fail (Beside (Quote n) (Beside (Text " does not fit in ") range)) =>
  InRange LT fromTop n range
else instance
  Fail (Beside (Quote n) (Beside (Text " does not fit in ") range)) =>
  InRange fromBottom GT n range
else instance InRange fromBottom fromTop n range
