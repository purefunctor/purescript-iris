module DeriveClauseRecovery where

data Empty = Empty
  derive ()

data Trailing = Trailing
  derive (Eq,)

data Missing = Missing
  derive (Eq Ord)

data Applied a = Applied a
  derive (Eq a)

data Bare = Bare
  derive Eq

value = 0
