-- @format width=25
-- @format width=25 unicode=true
module Main where

identity :: forall a. a -> a
identity = \value -> value
