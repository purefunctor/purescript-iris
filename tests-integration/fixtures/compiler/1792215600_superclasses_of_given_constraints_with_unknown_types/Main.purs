module Main where

import Data.Show (class Show, show)

class Container :: Type -> Type -> Constraint
class Show a <= Container a b

showsViaSuperclass :: forall a. Container a (Int String) => a -> String
showsViaSuperclass value = show value

type Invalid = Int String

class Constrained :: Type -> Constraint
class Constrained a

class Constrained Invalid <= Wrapper a

showsViaWrapper :: forall a. Wrapper a => a -> String
showsViaWrapper value = show value
