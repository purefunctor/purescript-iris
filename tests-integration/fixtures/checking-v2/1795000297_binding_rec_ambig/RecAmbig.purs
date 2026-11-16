module RecAmbig where

class Shw a where
  shw :: a -> String

class Def a where
  def :: a

k :: forall a b. a -> b -> a
k x _ = x

-- Recursive and ambiguous: which error does purs report first?
f x = k x (k (shw def) (f x))
