module Main where

type Invalid = Int String

class Global :: Type -> Type -> Constraint
class Global a b | a -> b

instance Global Int Invalid

needsGlobal :: Global Int Boolean => Int
needsGlobal = 0

usesGlobal :: Int
usesGlobal = needsGlobal

class Local :: Type -> Type -> Constraint
class Local a b | a -> b

needsLocal :: Local Int Boolean => Int
needsLocal = 0

usesLocal :: Local Int Invalid => Int
usesLocal = needsLocal
