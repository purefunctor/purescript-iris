module Data.Symbol where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

class IsSymbol :: Symbol -> Constraint
class IsSymbol sym where
  reflectSymbol :: Proxy sym -> String
