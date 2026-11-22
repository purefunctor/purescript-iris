module Main where

data Pair left right = Pair left right

infix 5 type Pair as <=>

infixl 6 type Pair as <+>
infixr 6 type Pair as <*>

validNonAssociative :: Int <=> Int -> Int
validNonAssociative _ = 0

ambiguousNonAssociative :: Int <=> Int <=> Int -> Int
ambiguousNonAssociative _ = 0

ambiguousMixedAssociativity :: Int <+> Int <*> Int -> Int
ambiguousMixedAssociativity _ = 0
