-- @format width=80
-- @format width=80 unicode=true
module Main where

applyTransformationTo :: forall input output. (input -> output) -> input -> output
applyTransformationTo = \transformFunction argument -> transformFunction argument
