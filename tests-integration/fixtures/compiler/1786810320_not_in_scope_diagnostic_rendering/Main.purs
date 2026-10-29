module Main where

-- Unresolved names in each lowering syntax position.
missingVariable = absent
missingConstructor = Missing
missingOperator = 1 + 2
missingNegate = -1

missingDo = do
  value <- absent
  value

missingDiscard = do
  absent
  absent

missingAdo = ado
  value <- absent
  second <- absent
  in value

missingPure = ado
  in 1

missingType :: MissingType
missingType = absent

missingPun = { absent }

type MissingTypeOperator = Int + String

infix 5 missingOperatorName as <+>
infix 5 type MissingTypeOperatorName as +
