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

argument = use [ do
  action, nextArgument ] finalArgument

patterns first second = case first, second of
  { firstField: firstValue, secondField: secondValue, thirdField: thirdValue, fourthField: fourthValue }, [ item, rest ] -> firstValue
  _, _ -> fallback

named whole@{ firstField: firstValue, secondField: secondValue, thirdField: thirdValue } = whole
lambda = \{ firstField: firstValue, secondField: secondValue, thirdField: thirdValue } -> firstValue

patternLocal = result
  where
  { firstField: result, secondField: secondValue, thirdField: thirdValue, fourthField: fourthValue } = source

doLocal = do
  let { firstField: result, secondField: secondValue, thirdField: thirdValue, fourthField: fourthValue } = source
  pure result

type Tail row = { | row }
type RowTail row = ( | row )
data Wrapped = Wrapped { firstField :: FirstType, secondField :: SecondType, thirdField :: ThirdType }

literal = [ """first
second""", { nested: [ { value: firstArgument }, { value: secondArgument }, { value: thirdArgument } ] } ]

updateEffect = record { field = do
  action, otherField = secondArgument }
qualifiedUpdate = M.record { field = firstArgument, otherField = secondArgument, lastField = thirdArgument }
accessUpdate = record.field { field = firstArgument, otherField = secondArgument, lastField = thirdArgument }
parenthesizedUpdate = (chooseRecord firstArgument) { field = firstArgument, otherField = secondArgument, lastField = thirdArgument }
sectionUpdate = _ { field = firstArgument, otherField = secondArgument, lastField = thirdArgument, extraField = fourthArgument }
widePrefix = use "界界界界" { field: do
  action, otherField: secondArgument }
