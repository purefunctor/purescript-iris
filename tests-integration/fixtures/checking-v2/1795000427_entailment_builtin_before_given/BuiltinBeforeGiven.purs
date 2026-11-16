module BuiltinBeforeGiven where

import Prim.Row (class Nub, class Union)

foreign import merge
  :: forall left right union result
   . Union left right union
  => Nub union result
  => Record left
  -> Record right
  -> Record result

withName
  :: forall rest duplicated result
   . Union (name :: String) rest duplicated
  => Nub (name :: String | rest) result
  => Record rest
  -> Record result
withName rest = merge { name: "" } rest
