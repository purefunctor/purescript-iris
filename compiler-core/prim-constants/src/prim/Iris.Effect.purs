module Iris.Effect where

class Has :: Type -> Row Type -> Constraint
class Has service services where
  getService :: Record services -> service

data Bundle :: Type -> Type -> Type
data Bundle left right

infixr 4 type Bundle as &
