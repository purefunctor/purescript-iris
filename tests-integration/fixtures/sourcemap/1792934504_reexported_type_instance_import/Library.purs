module Library (Box(..)) where

import Data.Show (class Show)

data Box = Box Int

instance showBox :: Show Box where
  show _ = "box"
