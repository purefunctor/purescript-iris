module NewtypeEquationDeriveClauses where

newtype Name = Name String
  derive (Eq, Ord)
  derive (Newtype)
