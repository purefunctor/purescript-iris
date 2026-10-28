module Main where

import Foreign.Object (Object)
import Foreign.Object as Object
import Foreign.Object.Unsafe as ObjectUnsafe

selected :: String -> Object String -> Array String
selected key object =
  if Object.member key object then [ ObjectUnsafe.unsafeIndex object key, ObjectUnsafe.unsafeIndex object "default" ]
  else Object.keys object
