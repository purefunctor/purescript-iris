module Lib where

value :: Int
value = 42

local :: { value :: Int }
local = { value }
