module Main where

import Data.Show (show)

type Invalid = Int String

class Convert :: Type -> Type -> Constraint
class Convert a b | a -> b where
  convert :: a -> b

instance Convert Int Int where
  convert value = value

showsConvert :: Invalid -> String
showsConvert value = show (convert value)

class Joint :: Type -> Type -> Type -> Constraint
class Joint a b c | a b -> c where
  joint :: a -> b -> c

instance Joint Int Int String where
  joint _ _ = "joint"

showsJoint :: Invalid -> String
showsJoint value = show (joint value 0)

class Independent :: Type -> Type -> Type -> Constraint
class Independent a b c | b -> c where
  independent :: a -> b -> c

instance Independent Int Int String where
  independent _ _ = "independent"

showsIndependent :: Invalid -> String
showsIndependent value = show (independent value 0)
