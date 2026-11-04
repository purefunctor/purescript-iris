module Capabilities where

import Iris.Effect (type (&))

newtype First = First Int

newtype Second = Second Int

newtype Tagged value = Tagged value

type Shop = First & Second
