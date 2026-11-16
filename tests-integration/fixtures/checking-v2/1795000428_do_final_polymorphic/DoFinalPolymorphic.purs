module DoFinalPolymorphic where

data Unit = Unit

data Box a = Box a

class Monoid m where
  mempty :: m

instance Monoid (Box Unit) where
  mempty = Box Unit

bind :: forall a b. Box a -> (a -> Box b) -> Box b
bind (Box a) f = f a

discard :: forall a b. Box a -> (a -> Box b) -> Box b
discard = bind

map :: forall a b. (a -> b) -> Box a -> Box b
map f (Box a) = Box (f a)

final :: Box Unit
final = do
  Box Unit
  mempty

branch :: Boolean -> Box Unit
branch condition = case condition of
  true -> Box Unit
  false -> do
    Box Unit
    mempty

applicative :: Box (Box Unit)
applicative = ado
  Box Unit
  in mempty
