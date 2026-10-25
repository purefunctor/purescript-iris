module Main where

import Prelude

import Lookalike (Wrapped)
import Lookalike as Lookalike

partiallyAppliedEqual :: Int -> Boolean
partiallyAppliedEqual = eq 1

partiallyAppliedLessThan :: Int -> Boolean
partiallyAppliedLessThan = (<) 1

genericEqual :: forall value. Eq value => value -> value -> Boolean
genericEqual left right = left == right

genericLessThan :: forall value. Ord value => value -> value -> Boolean
genericLessThan left right = left < right

unitEqual :: Unit -> Unit -> Boolean
unitEqual left right = left == right

wrappedEqual :: Wrapped -> Wrapped -> Boolean
wrappedEqual left right = left == right

wrappedGreaterThan :: Wrapped -> Wrapped -> Boolean
wrappedGreaterThan left right = left > right

lookalikeLessThan :: Int -> Int -> Boolean
lookalikeLessThan left right = Lookalike.lessThan left right
