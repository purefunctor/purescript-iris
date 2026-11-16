module Main where

import Data.Reflectable (reflectType)
import Data.Symbol (Proxy(..), reflectSymbol)
import Prim.Boolean (True)
import Prim.Ordering (GT)

symbolName = reflectSymbol (Proxy :: Proxy "hello")

reflectedSymbol = reflectType (Proxy :: Proxy "world")

reflectedInt = reflectType (Proxy :: Proxy 42)

reflectedBoolean = reflectType (Proxy :: Proxy True)

reflectedOrdering = reflectType (Proxy :: Proxy GT)

notReflectable = reflectType (Proxy :: Proxy Int)
