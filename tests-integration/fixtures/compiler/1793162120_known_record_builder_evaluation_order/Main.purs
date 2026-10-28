module Main where

import Prelude

import Record.Builder as Builder
import Type.Proxy (Proxy(..))

foreign import observe :: forall value. String -> value -> value
foreign import readTrace :: Boolean -> Array String

flipped :: { x :: Int } -> { x :: Int, y :: String, z :: Boolean }
flipped record =
  Builder.build
    ( Builder.insert (Proxy :: _ "y") (observe "y" "y")
        >>> Builder.modify (Proxy :: _ "x") (observe "modify" (\x -> observe "apply" (x + 1)))
        >>> Builder.merge { z: observe "z" true }
    )
    (observe "record" record)

composed :: { x :: Int } -> { x :: Int, y :: String }
composed record =
  Builder.build
    ( Builder.insert (Proxy :: _ "y") (observe "y" "y")
        <<< Builder.modify (Proxy :: _ "x") (\x -> observe "apply" (x + 1))
    )
    (observe "record" record)

literal :: Boolean -> { x :: Int, y :: Int }
literal _ =
  Builder.build
    ( Builder.modify (Proxy :: _ "y") (\y -> observe "first" (y + 1))
        >>> Builder.modify (Proxy :: _ "x") (\x -> observe "second" (x + 1))
        >>> Builder.delete (Proxy :: _ "z")
    )
    { x: observe "x" 1, y: observe "y" 2, z: observe "z" 3 }
