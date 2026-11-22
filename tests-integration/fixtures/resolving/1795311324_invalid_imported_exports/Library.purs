module Library (ByteCount(..), helper, class Helper, (+), type (:+)) where

newtype ByteCount = ByteCount Int

helper :: Int -> Int -> Int
helper a _ = a

class Helper a

infixl 6 helper as +

type Pair a b = a

infixl 6 type Pair as :+
