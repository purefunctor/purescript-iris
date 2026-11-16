module Main where

import Lib as Lib

class Equal a
--    $@%&

data Colour = Red | Green
  derive (Equal, Lib.Show)
--        $@%&   $@%&
  derive (Equal)
--          %

derive instance Equal Colour
--               &
