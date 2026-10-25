module Main (chosen) where

import Library hiding (select) as Empty
import Library (select) as Qualified
import Library (select)

chosen :: Int
chosen = Qualified.select 3 9
