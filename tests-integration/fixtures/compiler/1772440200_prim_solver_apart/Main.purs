module Main where

import Prim.Int as Int
import Prim.Ordering (LT)
import Prim.Symbol as Symbol
import Type.Proxy (Proxy(..))

invalidAdd :: Int.Add 2 3 10 => Proxy 10
invalidAdd = Proxy

invalidCompare :: Int.Compare 5 1 LT => Proxy LT
invalidCompare = Proxy

invalidAppend :: Symbol.Append "hello" "world" "xyz" => Proxy "xyz"
invalidAppend = Proxy

invalidCons :: Symbol.Cons "hello" "world" "helloworld" => Proxy "world"
invalidCons = Proxy

forceSolve =
  { invalidAdd
  , invalidCompare
  , invalidAppend
  , invalidCons
  }

forceSolveChecked :: { invalidCompare :: Proxy LT, invalidCons :: Proxy "world" }
forceSolveChecked =
  { invalidCompare
  , invalidCons
  }
