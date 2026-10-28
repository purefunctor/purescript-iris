module Main where

import Data.Generic.Rep (class Generic)
import Lib (Wrapper)

foreign import data Opaque :: Type

derive instance Generic Wrapper _

derive instance Generic Opaque _
