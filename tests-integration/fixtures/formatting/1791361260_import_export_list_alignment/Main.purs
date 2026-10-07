-- @format width=52
-- @format width=53
-- @format width=80
-- @format width=30 indent=4
-- @format width=20 indent=4
module Main (first, second, Box(First, Second)) where

import Library (first, second, Box(First, Second))
import Library hiding (first, second, Box(First, Second)) as Hidden

first = 1
second = 2
data Box = First | Second
