module OperatorChainIncomplete where

topLevel n = n +

letEquation n =
  let
    m = n +
    Tuple a b = n +
  in
    m

doBind x = do
  y <- x +
  pure (x +)

guardBind x
  | Just y <- x + = y
  | x + = x

adjacent a b = a + * b

missingHead = (+ 1)

tickChain a b = a `f +` b

tickAdjacent a b = a `f + * g` b

infixMissing a = a `f`

infixHead b = `f` b

type TypeOp = A +

type TypeOpHead = (+ A)

binderOp (x :) = x

binderOpHead (: xs) = xs
