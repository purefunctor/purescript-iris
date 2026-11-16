module WildcardSignatures where

class Shw :: Type -> Constraint
class Shw a where
  shw :: a -> String

instance Shw Int where
  shw _ = "Int"

-- Wildcards the body leaves unsolved are generalised.
same :: _ -> _
same x = x

-- A wildcard in an expression annotation or a let signature is solved by inference.
annotated :: Int
annotated = (1 :: _)

local :: Int
local = let y :: _
            y = 2
        in y

-- A partial signature does not generalise constraints, as with any signature.
shown :: _ -> String
shown x = shw x
