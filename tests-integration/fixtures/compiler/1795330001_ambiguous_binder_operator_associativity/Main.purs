module Main where

data Pair left right = Pair left right

infix 5 Pair as <=>

infixl 6 Pair as <+>
infixr 6 Pair as <*>

validNonAssociative :: Pair Int Int -> Int
validNonAssociative (left <=> _) = left

ambiguousNonAssociative :: Pair (Pair Int Int) Int -> Int
ambiguousNonAssociative (left <=> _ <=> _) = left

ambiguousMixedAssociativity :: Pair (Pair Int Int) Int -> Int
ambiguousMixedAssociativity (left <+> _ <*> _) = left
