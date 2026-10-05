-- @format width=80
-- @format width=20 indent=1
-- @format width=40 indent=4 unicode=true
module Main where

infixl 4
  -- fixity explanation
  add as +

derive
  -- derive explanation
  instance Eq Foo

type role Foo
  -- role explanation
  nominal
