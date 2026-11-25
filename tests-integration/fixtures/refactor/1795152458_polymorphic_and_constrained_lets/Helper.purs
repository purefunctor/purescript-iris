module Helper
  ( Box(..)
  , class BoxLike
  , boxMap
  , pickFirst
  ) where

data Box a = Box a

class BoxLike f where
  boxMap :: forall a b. (a -> b) -> f a -> f b

instance BoxLike Box where
  boxMap f (Box a) = Box (f a)

pickFirst :: forall a b. a -> b -> a
pickFirst first _ = first
