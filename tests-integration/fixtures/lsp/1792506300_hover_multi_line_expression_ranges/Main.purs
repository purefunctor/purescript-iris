module Main where

conditional flag =
  -- | Leading comments are not part of the expression.
  if flag
--$
    then 1
    else 2

scrutinise value =
  case value of
--$
    0 -> "zero"
    _ -> "other"

binding =
  let
--$
    life = 42
  in
    life
