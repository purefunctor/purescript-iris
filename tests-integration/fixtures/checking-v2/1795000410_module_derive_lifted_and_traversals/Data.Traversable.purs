module Data.Traversable where

import Data.Foldable (class Foldable)

class Traversable :: (Type -> Type) -> Constraint
class Foldable t <= Traversable t
