module Iris.Effect where

class Has :: Type -> Row Type -> Constraint
class Has service services where
  getService :: Record services -> service
