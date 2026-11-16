module DeriveFunctor where

import Data.Functor (class Functor, map)

data T a = T a (Array a) { r :: a, n :: Int }

derive instance functorT :: Functor T

foreign import plusOne :: Int -> Int

out :: T Int
out = map plusOne (T 1 [2, 3] { r: 4, n: 10 })
