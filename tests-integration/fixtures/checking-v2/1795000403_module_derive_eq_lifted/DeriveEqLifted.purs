module DeriveEqLifted where

import Data.Eq (class Eq, class Eq1)

data Wrap f a = Wrap (f a) { inner :: f Int }

derive instance eqWrap :: (Eq1 f, Eq a) => Eq (Wrap f a)

data Missing f a = Missing (f a)

derive instance eqMissing :: Eq a => Eq (Missing f a)
