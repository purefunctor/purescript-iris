module DataEquationDeriveClauses where

data Colour = Red | Green | Blue
  derive (Eq, Ord)
  derive (Generic)

data List a = Nil | Cons a (List a) derive (Functor, F.Foldable)

data Void
  derive (Eq)
