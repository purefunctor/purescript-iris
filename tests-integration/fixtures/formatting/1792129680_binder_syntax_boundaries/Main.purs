-- @format width=80 incomplete=false
-- @format width=100 indent=4 unicode=true
module Main where

binders {} [] namedBinder@wholeValue (annotatedItem :: Int) { recordField: recordField } = wholeValue
binderOperator (firstElement : secondElement : thirdElement : fourthElement : remainingElements) = firstElement
