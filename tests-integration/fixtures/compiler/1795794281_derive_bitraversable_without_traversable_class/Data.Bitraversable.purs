module Data.Bitraversable where

import Control.Applicative (class Applicative)

class Bitraversable :: (Type -> Type -> Type) -> Constraint
class Bitraversable t where
  bitraverse :: forall f a b c d. Applicative f => (a -> f c) -> (b -> f d) -> t a b -> f (t c d)
  bisequence :: forall f a b. Applicative f => t (f a) (f b) -> f (t a b)
