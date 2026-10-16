-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

qualifiedUpdate = M.record { field = firstArgument, otherField = secondArgument, lastField = thirdArgument }
accessUpdate = record.field { field = firstArgument, otherField = secondArgument, lastField = thirdArgument }
parenthesizedUpdate = (chooseRecord firstArgument) { field = firstArgument, otherField = secondArgument, lastField = thirdArgument }
sectionUpdate = _ { field = firstArgument, otherField = secondArgument, lastField = thirdArgument, extraField = fourthArgument }
