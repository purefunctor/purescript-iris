module Main where

data Box a = Box a

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

foreign import data Q :: forall k. k

class Foo a where
  foo :: a -> Int

class Bar a b where
  bar :: a -> b -> Int

class Pk :: forall k. k -> Constraint
class Pk a where
  pk :: Proxy a -> Int

wrap :: forall k (a :: k). Proxy a -> Proxy (Q :: k)
wrap _ = Proxy

nested x = foo (Box x)

bare x = foo x

mixed x y = bar (Box x) y

kinded proxy = pk (wrap proxy)
