module Lib where

-- | Adds one.
increment :: Int -> Int
increment value = value

combine :: Int -> Int -> Int
combine left _ = left

infixl 4 combine as <.>

data Box a = Box a

longKinded :: forall (f :: Type -> Type) a b. f a -> f b -> f a -> f b -> f a -> f b -> f a -> f a
longKinded value _ _ _ _ _ _ = value
