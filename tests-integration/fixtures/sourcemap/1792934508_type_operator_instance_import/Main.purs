module Main (described) where

import Prelude
import Operators (type (:*:))
import Config (value)

selected :: Int :*: Int
selected = value

described :: String
described = show selected
