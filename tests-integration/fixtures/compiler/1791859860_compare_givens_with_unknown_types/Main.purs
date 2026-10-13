module Main where

import Prim.Int (class Compare)
import Prim.Ordering (LT)
import Type.Proxy (Proxy)

type Invalid :: Int
type Invalid = 1 2

needsLess :: forall m p. Compare m p LT => Proxy m -> Proxy p -> Int
needsLess _ _ = 0

usesInvalidGivens :: forall m p. Compare m Invalid LT => Compare Invalid p LT => Proxy m -> Proxy p -> Int
usesInvalidGivens m p = needsLess m p

usesNoGivens :: forall m p. Proxy m -> Proxy p -> Int
usesNoGivens m p = needsLess m p
