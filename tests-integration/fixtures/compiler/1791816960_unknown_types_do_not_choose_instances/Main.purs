module Main where

type Invalid = Int String

class Context :: Type -> Constraint
class Context a

instance Context Int

class Wrapper :: Type -> Constraint
class Wrapper a

instance Context a => Wrapper (Array a)

needsWrapper :: Wrapper Invalid => Int
needsWrapper = 0

usesWrapper :: Int
usesWrapper = needsWrapper
