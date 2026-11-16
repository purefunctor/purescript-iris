module GenOrder where

class Eq1 a where
  eq1 :: a -> a -> Boolean

class Eq1 a <= Ord1 a where
  lt1 :: a -> a -> Boolean

eqThenOrd x y = if eq1 x y then lt1 x y else false
ordThenEq x y = if lt1 x y then eq1 x y else false
