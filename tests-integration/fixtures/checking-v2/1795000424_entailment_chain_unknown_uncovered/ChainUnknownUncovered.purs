module ChainUnknownUncovered where

import Prim.Int (class Add)

foreign import unsafeCoerce :: forall a b. a -> b

data Proxy :: forall k. k -> Type
data Proxy a = Proxy

data Box a = Box a

class Unpad :: Int -> Type -> Type -> Constraint
class Unpad n a b | n b -> a where
  unpad :: Proxy n -> b -> a

instance Unpad (-1) a a where
  unpad _ x = x
else instance (Unpad n (Box (Box a)) b, Add n 1 n1) => Unpad n1 (Box a) b where
  unpad _ = unsafeCoerce

test :: Box (Box (Box Int)) -> Box Int
test x = unpad (Proxy :: Proxy 1) x

-- n is never determined, and Int never matches a Box, so the chain must wait
-- instead of unfolding the second instance forever.
undetermined :: Int -> Box Int
undetermined x = unpad Proxy x
