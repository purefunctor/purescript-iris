module Main where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

keepProxy :: forall k (a :: k). Proxy a -> Int
keepProxy _ = 0

concreteProxy :: Int
concreteProxy =
  let
    hello = Proxy :: Proxy "hello"
  in
    keepProxy hello

explicitKind :: forall k (a :: k). Proxy a -> Int
explicitKind p =
  let
    same = p
  in
    keepProxy same

class Poly :: forall k. k -> Constraint
class Poly a where
  poly :: Proxy a -> Int

instance Poly a where
  poly _ =
    let
      same = Proxy :: Proxy a
    in
      keepProxy same

data HoldProxy :: forall k. (k -> Type) -> Type
data HoldProxy f = HoldProxy

takeSymbolHold :: HoldProxy (Proxy :: Symbol -> Type) -> Int
takeSymbolHold _ = 0

underAppliedProxy :: HoldProxy (Proxy :: Symbol -> Type) -> Int
underAppliedProxy h =
  let
    same = h
  in
    takeSymbolHold same

takeEmptyRowTypeProxy :: Proxy (() :: Row Type) -> Int
takeEmptyRowTypeProxy _ = 0

emptyRowProxyUse :: Int
emptyRowProxyUse =
  let
    emptyRow = Proxy :: Proxy (() :: Row Type)
  in
    takeEmptyRowTypeProxy emptyRow
