module Main where

import Prelude

import Data.Maybe (Maybe(..))
import Data.Maybe as Maybe

describe :: Maybe Int -> String
describe value = Maybe.maybe "none" show value

present :: Maybe Int
present = Just 1
