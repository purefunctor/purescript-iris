module Main where

import Prim.TypeError (class Fail, Text)

type Invalid = Int String

class Chain :: Type -> Type -> Constraint
class Chain a b

instance Chain Int b
else instance Fail (Text "Chain fell through to its last instance") => Chain a b

needsChain :: Chain Invalid Boolean => Int
needsChain = 0

usesChain :: Int
usesChain = needsChain
