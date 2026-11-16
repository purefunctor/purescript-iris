module Convert where

class Convert :: Type -> Type -> Constraint
class Convert a b | a -> b

class Pair :: Type -> Type -> Constraint
class Pair a b
