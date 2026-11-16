module Main where

data Maybe a = Nothing | Just a

newtype Box a = Box a

class Show a where
  show :: a -> String

instance Show Int where
  show _ = "int"

instance Show a => Show (Maybe a) where
  show Nothing = "nothing"
  show (Just a) = show a

identity :: forall a. a -> a
identity x = x

constant x _ = x

fromMaybe default m = case m of
  Nothing -> default
  Just a -> a

showTwice x = show x `append` show x
  where
  append :: String -> String -> String
  append a _ = a

describe = show (Just 1)

record = { a: 1, b: "s" }

access r = r.a
