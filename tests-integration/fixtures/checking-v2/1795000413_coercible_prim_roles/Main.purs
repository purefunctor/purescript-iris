module Main where

import Prim.Coerce (class Coercible)

foreign import unsafeCoerce :: forall a b. a -> b

coerce :: forall a b. Coercible a b => a -> b
coerce = unsafeCoerce

newtype Age = Age Int

functions :: (Int -> Int) -> (String -> String)
functions = coerce

arrays :: Array Int -> Array String
arrays = coerce

records :: Record (name :: Int) -> Record (name :: String)
records = coerce

ages :: Array (Age -> Int) -> Array (Int -> Age)
ages = coerce

agedRecords :: { age :: Age } -> { age :: Int }
agedRecords = coerce
