module Main where

class Semigroupoid a where
  compose :: forall b c d. a c d -> a b c -> a b d

instance Semigroupoid Function where
  compose f g x = f (g x)

toString :: Int -> String
toString _ = "x"

toBool :: String -> Boolean
toBool _ = true

combine :: Int -> Function String Boolean
combine _ s = toBool s

example :: Boolean
example =
  let
    composed = compose toBool toString
    curried = compose combine (\n -> n)
  in
    curried 1 (toString 2) && composed 3

infixr 3 and as &&

and :: Boolean -> Boolean -> Boolean
and true b = b
and false _ = false
