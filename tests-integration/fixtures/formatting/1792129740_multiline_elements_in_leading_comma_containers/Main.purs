-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

effect = do
  [ do
      firstAction
    , ado
      value <- secondAction
      in value
    ]
  { result: case input of
      Just value -> value
      Nothing -> fallback
  , local: let value = firstArgument in value
  , choice: if conditionHolds firstArgument then transform firstArgument else transform secondArgument
  }
  finalAction

literal = [ """first
second""", { nested: [ { value: firstArgument }, { value: secondArgument }, { value: thirdArgument } ] } ]
