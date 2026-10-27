module Class where

class Convert a b | a -> b where
  convert :: a -> b
