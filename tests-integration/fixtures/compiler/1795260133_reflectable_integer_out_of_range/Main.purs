module Main where

import Data.Reflectable (reflectType)
import Type.Proxy (Proxy(..))

maximum :: Int
maximum = reflectType (Proxy :: Proxy 2147483647)

minimum :: Int
minimum = reflectType (Proxy :: Proxy (-2147483648))

aboveMaximum :: Int
aboveMaximum = reflectType (Proxy :: Proxy 2147483648)

beyondInt64 :: Int
beyondInt64 = reflectType (Proxy :: Proxy 99999999999999999999)
