module Main where

type Invalid = Int String

class Determined :: Type -> Type -> Constraint
class Determined a b | a -> b

instance Determined Int Boolean

needsDetermined :: Determined Int Invalid => Int
needsDetermined = 0

usesDetermined :: Int
usesDetermined = needsDetermined

class Unavailable :: Constraint
class Unavailable

class Required :: Type -> Type -> Constraint
class Required a b | a -> b

instance Unavailable => Required Int Boolean

needsRequired :: Required Int Invalid => Int
needsRequired = 0

usesRequired :: Int
usesRequired = needsRequired
