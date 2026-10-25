module Main (described, fallback) where

import Prelude
import Data.Show (class Show)
import Data.Maybe hiding (fromMaybe)

described :: Int -> String
described = show

fallback :: Maybe Int -> Int
fallback = maybe 0 identity
