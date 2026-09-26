module Shapes where

class Area a where
  area :: a -> Int

data Box a = Box a

newtype Size = Size Int

instance Area Int where
  area value = value

instance Area (Box a) where
  area _ = 1

instance Area (Array (Box a)) where
  area _ = 2

derive newtype instance Area Size
