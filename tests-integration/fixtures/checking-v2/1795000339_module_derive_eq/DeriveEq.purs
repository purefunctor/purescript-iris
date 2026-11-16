module DeriveEq where

import Data.Eq (class Eq, eq)

data T a = A a | B Int { x :: a, y :: Int }

derive instance eqT :: Eq a => Eq (T a)

same :: Boolean
same = eq (B 1 { x: 1, y: 2 }) (B 1 { x: 1, y: 2 })

differ :: Boolean
differ = eq (B 1 { x: 1, y: 2 }) (B 1 { x: 1, y: 3 })
