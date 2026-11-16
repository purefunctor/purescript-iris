module DeriveFunctorContra where

import Data.Functor (class Functor)

-- `a` occurs contravariantly, so there is no lawful map.
data F a = F (a -> Int)

derive instance functorF :: Functor F
