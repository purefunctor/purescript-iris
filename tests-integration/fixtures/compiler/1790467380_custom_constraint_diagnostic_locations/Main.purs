module Main where

import Prim.TypeError (class Warn, Beside, Quote, Text)

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

data Pair a b = Pair a b

warned :: forall a. Warn (Text "warned was used") => a -> a
warned x = x

test :: Pair Int Int
test = Pair (warned 1) (warned 2)

class Describe a where
  describe :: a -> Int

data Legacy = Legacy

instance Warn (Text "Legacy is deprecated") => Describe Legacy where
  describe _ = 0

test2 :: Int
test2 = describe Legacy

data Key = Key

class Resolve :: Type -> Type -> Constraint
class Resolve a b | a -> b

instance Resolve Key Int

class Lookup :: Type -> Type -> Constraint
class Lookup a b

instance Resolve k v => Lookup (Proxy k) v

lookup :: forall b. Lookup (Proxy Key) b => Proxy b
lookup = Proxy

warnResolved :: forall b. Warn (Beside (Text "Resolved to ") (Quote b)) => Proxy b -> Proxy b
warnResolved p = p

consume :: forall a. Proxy a -> Int
consume _ = 0

test3 :: Int
test3 = consume (warnResolved lookup)
