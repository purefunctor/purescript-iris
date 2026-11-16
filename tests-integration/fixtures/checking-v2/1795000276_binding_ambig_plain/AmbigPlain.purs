module AmbigPlain where

class Shw a where
  shw :: a -> String

class Def a where
  def :: a

k :: forall a b. a -> b -> a
k x _ = x

-- Shw ?c and Def ?c mention an unknown the type does not determine.
test x = k x (shw def)
