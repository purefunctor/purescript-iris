module Main where

import Data.Generic.Rep (class Generic, from)

data Shape a = Circle a | Rectangle a a Int | Empty

derive instance genericShape :: Generic (Shape a) _

data Void

derive instance genericVoid :: Generic Void _

represented = from (Rectangle 1 2 3)
