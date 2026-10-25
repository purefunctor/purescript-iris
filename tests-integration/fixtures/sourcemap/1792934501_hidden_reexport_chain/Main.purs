module Main (chosen) where

import Bridge hiding (select)
import Facade (select) as Qualified
import Bridge (select)

chosen :: Int
chosen = Qualified.select 3 9
