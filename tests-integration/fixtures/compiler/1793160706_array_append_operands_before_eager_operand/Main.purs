module Main where

import Prelude

foreign import traced :: forall a. String -> a -> a

appendBeforeCase :: Array Int -> Int -> Array Int
appendBeforeCase values count =
  [ traced "literal" 0 ] <> traced "spread" values <> case traced "scrutinee" count of
    0 -> []
    _ -> traced "nonempty" [ count ]
