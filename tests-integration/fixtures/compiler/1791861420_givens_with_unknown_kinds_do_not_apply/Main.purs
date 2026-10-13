module Main where

type Invalid :: Type
type Invalid = Int String

class Global :: forall kind. kind -> Constraint
class Global a

instance Global (Int :: Invalid)

needsGlobal :: Global Int => Int
needsGlobal = 0

usesGlobal :: Int
usesGlobal = needsGlobal

class Local :: forall kind. kind -> Constraint
class Local a

needsLocal :: Local Int => Int
needsLocal = 0

usesLocal :: Local (Int :: Invalid) => Int
usesLocal = needsLocal
