module Main where

type Invalid = Int String

class Global :: Type -> Constraint
class Global a where
  fromGlobal :: a -> Int

instance Global Invalid where
  fromGlobal _ = 0

usesGlobal :: Int
usesGlobal = fromGlobal 0

class Local :: Type -> Constraint
class Local a where
  fromLocal :: a -> Int

usesLocal :: Local Invalid => Int
usesLocal = fromLocal 0

usesSkolemLocal :: forall a. Local Invalid => a -> Int
usesSkolemLocal value = fromLocal value
