module KindLeak where

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

-- D and E form one data binding group. E's second parameter has a kind
-- that nothing fixes; D mentions E without fixing it either.
data D = D (Proxy E)
data E a b = E D
