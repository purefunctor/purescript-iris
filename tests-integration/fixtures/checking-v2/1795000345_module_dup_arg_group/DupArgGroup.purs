module DupArgGroup where

data T a a = T (U a)
data U a = U (T a a)
