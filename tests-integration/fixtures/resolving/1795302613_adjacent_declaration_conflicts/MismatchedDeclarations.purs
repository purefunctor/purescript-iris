module MismatchedDeclarations where

type Alias = Int
data Alias = Alias

foreign import external :: Int
external = 0

class Shape a
data Shape = Circle
