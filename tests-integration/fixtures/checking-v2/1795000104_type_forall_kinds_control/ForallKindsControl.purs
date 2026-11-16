module ForallKindsControl where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

-- Same pair, but the body mentions the variable through a poly-kinded constructor (Proxy @k a),
-- so the two kinds meet in a KindApp: rejected.
x :: { f :: forall (a :: Type). Proxy a -> Int }
x = { f: \_ -> 0 }

y :: { f :: forall (a :: Symbol). Proxy a -> Int }
y = { f: \_ -> 1 }

arr = [x, y]
