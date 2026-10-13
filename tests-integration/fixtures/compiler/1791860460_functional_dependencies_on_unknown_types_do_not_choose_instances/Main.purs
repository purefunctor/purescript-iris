module Main where

type Invalid = Int String

class Convert :: Type -> Type -> Constraint
class Convert a b | a -> b where
  convert :: a -> b

instance Convert Int Int where
  convert value = value

instance Convert String Boolean where
  convert _ = true

usesConvert :: Invalid -> Boolean
usesConvert value = convert value

class Bidirectional :: Type -> Type -> Constraint
class Bidirectional a b | a -> b, b -> a

instance Bidirectional Int String

needsBidirectional :: Bidirectional Invalid Boolean => Int
needsBidirectional = 0

usesBidirectional :: Int
usesBidirectional = needsBidirectional
