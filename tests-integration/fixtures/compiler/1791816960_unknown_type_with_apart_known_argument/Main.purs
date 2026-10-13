module Main where

type Invalid = Int String

class Known :: Type -> Type -> Constraint
class Known a b

instance Known Int Int

needsKnown :: Known Invalid Boolean => Int
needsKnown = 0

usesKnown :: Int
usesKnown = needsKnown
