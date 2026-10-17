module Main where

class Pick :: Type -> Constraint
class Pick a where
  pick :: a -> String

instance Pick Boolean where
  pick _ = "boolean"
else instance Pick (Array (Int String)) where
  pick _ = "invalid"
else instance Pick a where
  pick _ = "other"

picksBoolean :: String
picksBoolean = pick true

picksString :: String
picksString = pick "string"

picksArray :: String
picksArray = pick [1]

class Single :: Type -> Constraint
class Single a where
  single :: a -> String

instance Single (Int String) where
  single _ = "invalid"

instance Single Boolean where
  single _ = "boolean"

singleInt :: String
singleInt = single 1

singleBoolean :: String
singleBoolean = single true
