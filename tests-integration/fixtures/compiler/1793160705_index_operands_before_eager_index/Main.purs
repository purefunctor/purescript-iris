module Main where

import Prelude

import Data.Array (unsafeIndex)
import Partial.Unsafe (unsafePartial)

foreign import traced :: forall a. String -> a -> a

indexBeforeCase :: Array String -> Int -> String
indexBeforeCase values position = unsafePartial
  ( unsafeIndex (traced "array" values) case traced "scrutinee" position of
      0 -> traced "first" 0
      _ -> traced "last" 2
  )
