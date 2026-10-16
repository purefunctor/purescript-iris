-- @format width=80
-- @format width=100 indent=4 unicode=true
module Main where

instance
  -- unnamed instance explanation
  Show Int where
    show value = "value"

instance named
  -- named instance explanation
  :: Eq Int where
    eq first second = true
