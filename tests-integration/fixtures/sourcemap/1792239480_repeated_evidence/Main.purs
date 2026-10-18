module Main where

import Prelude

showBoth :: forall a. Show a => Array a -> Array a -> String
showBoth first second =
  show first <> show second

showIntegers :: Array Int -> Array Int -> String
showIntegers first second =
  show first <> show second
